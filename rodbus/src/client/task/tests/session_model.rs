// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! A client session against generated replies and settings, checked step by step against a model
//! of the request each step writes, the result it reports, and how the session ends
//!
//! Every request reads the holding register at 0x10 with [`TIMEOUT`]. The peer writes only after
//! it read the request, and every wait is a multiple of [`TICK`], which [`TIMEOUT`] isn't, so no
//! input arrives at the instant a deadline expires. A session the model expects to be running
//! after the last step must answer one more request, then end when the channel is dropped.

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;

use super::*;
use crate::decode::{FrameDecodeLevel, PhysDecodeLevel};
use crate::error::{AduParseError, FrameParseError};
use crate::exception::ExceptionCode;

/// Every wait is a multiple of this, and [`TIMEOUT`] isn't
const TICK: Duration = Duration::from_millis(7);

/// The first multiple of [`TICK`] past [`TIMEOUT`]
const PAST_TIMEOUT: Duration = Duration::from_millis(7 * 143);

/// The address of the register every request reads
const REGISTER: u16 = 0x10;

/// The framing a session runs over
#[derive(Copy, Clone, Debug, PartialEq)]
enum Link {
    Tcp,
    #[cfg(feature = "serial")]
    Rtu,
}

/// What the peer does once it read a request
#[derive(Copy, Clone, Debug)]
enum Reply {
    /// the response with this value
    Answer(u16),
    /// an exception response with this code, and the exception the client reports for it
    Exception(u8, ExceptionCode),
    /// a response with another function code: one to a read of coils
    WrongFunction,
    /// a response with another value from the next unit up, then the answer
    OtherUnitThenAnswer(u16),
    /// a response with another value and the previous transaction id, then the answer; TCP only,
    /// since RTU frames carry no transaction id
    StaleTxThenAnswer(u16),
    /// nothing, so the request times out
    Silence,
    /// nothing until the request timed out, then the answer, while no request is outstanding
    LateAfterTimeout(u16),
    /// a frame the client can't parse, which ends the session: a non-Modbus protocol id over TCP,
    /// a wrong CRC over RTU
    Garbage,
}

/// A setting changed through the channel
#[derive(Copy, Clone, Debug)]
enum Change {
    DecodeLevel(DecodeLevel),
    Enable,
    Disable,
}

#[derive(Copy, Clone, Debug)]
enum Step {
    /// a read of [`REGISTER`] from `unit`, and what the peer does
    Request {
        unit: u8,
        reply: Reply,
    },
    Setting(Change),
}

/// The kinds of steps, to check the generator reaches each one
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Answer,
    Exception,
    WrongFunction,
    OtherUnitThenAnswer,
    StaleTxThenAnswer,
    Silence,
    LateAfterTimeout,
    Garbage,
    DecodeLevel,
    Enable,
    Disable,
}

impl Step {
    fn kind(self) -> Kind {
        match self {
            Step::Request { reply, .. } => match reply {
                Reply::Answer(_) => Kind::Answer,
                Reply::Exception(..) => Kind::Exception,
                Reply::WrongFunction => Kind::WrongFunction,
                Reply::OtherUnitThenAnswer(_) => Kind::OtherUnitThenAnswer,
                Reply::StaleTxThenAnswer(_) => Kind::StaleTxThenAnswer,
                Reply::Silence => Kind::Silence,
                Reply::LateAfterTimeout(_) => Kind::LateAfterTimeout,
                Reply::Garbage => Kind::Garbage,
            },
            Step::Setting(Change::DecodeLevel(_)) => Kind::DecodeLevel,
            Step::Setting(Change::Enable) => Kind::Enable,
            Step::Setting(Change::Disable) => Kind::Disable,
        }
    }
}

/// How a session ended
#[derive(Debug, PartialEq)]
enum Ending {
    /// it was still running after the last step, and answered one more request
    Running,
    Ended(SessionError),
}

/// What the model expects of a request
#[derive(Debug, PartialEq)]
struct Outcome {
    result: Result<Vec<Indexed<u16>>, RequestError>,
    /// how long after the request was made its result is reported
    elapsed: Duration,
}

/// The session as the model sees it
struct Model {
    link: Link,
    max_timeouts: Option<usize>,
    /// the transaction id of the next request, whatever became of the earlier ones
    next_tx_id: u16,
    /// response timeouts since the session started or since a request ended otherwise
    timeouts: usize,
}

impl Model {
    fn new(link: Link, max_timeouts: Option<usize>) -> Self {
        Self {
            link,
            max_timeouts,
            next_tx_id: 0,
            timeouts: 0,
        }
    }

    /// No frame answers a broadcast, which RTU sends to address 0; TCP has no broadcast
    fn is_broadcast(&self, unit: u8) -> bool {
        unit == 0 && self.link != Link::Tcp
    }

    /// The transaction id a request is written with, its outcome, and how the session ends with
    /// it, if it does
    fn request(&mut self, unit: u8, reply: Reply) -> (u16, Outcome, Option<SessionError>) {
        let tx_id = self.next_tx_id;
        self.next_tx_id = tx_id.wrapping_add(1);
        let result = match reply {
            Reply::Garbage => {
                let (_, error) = garbage(self.link, tx_id, unit);
                let outcome = Outcome {
                    result: Err(RequestError::BadFrame(error)),
                    elapsed: Duration::ZERO,
                };
                return (tx_id, outcome, Some(SessionError::BadFrame));
            }
            _ if self.is_broadcast(unit) => Err(RequestError::ResponseTimeout),
            Reply::Silence | Reply::LateAfterTimeout(_) => Err(RequestError::ResponseTimeout),
            Reply::Answer(value)
            | Reply::OtherUnitThenAnswer(value)
            | Reply::StaleTxThenAnswer(value) => Ok(vec![Indexed::new(REGISTER, value)]),
            Reply::Exception(_, exception) => Err(RequestError::Exception(exception)),
            Reply::WrongFunction => Err(RequestError::BadResponse(
                AduParseError::UnknownResponseFunction(0x01, 0x03, 0x83),
            )),
        };
        if result == Err(RequestError::ResponseTimeout) {
            self.timeouts = self.timeouts.saturating_add(1);
            let ends = self
                .max_timeouts
                .filter(|max| self.timeouts >= *max)
                .map(SessionError::MaxTimeouts);
            let outcome = Outcome {
                result,
                elapsed: TIMEOUT,
            };
            (tx_id, outcome, ends)
        } else {
            self.timeouts = 0;
            let outcome = Outcome {
                result,
                elapsed: Duration::ZERO,
            };
            (tx_id, outcome, None)
        }
    }

    /// How the session ends with a setting, if it does
    fn setting(&self, change: Change) -> Option<SessionError> {
        match change {
            Change::DecodeLevel(_) | Change::Enable => None,
            Change::Disable => Some(SessionError::Disabled),
        }
    }

    /// How the session ends with a step, if it does
    fn step(&mut self, step: Step) -> Option<SessionError> {
        match step {
            Step::Request { unit, reply } => self.request(unit, reply).2,
            Step::Setting(change) => self.setting(change),
        }
    }
}

/// The steps a session of `steps` runs, through the one that ends it, and how the model expects
/// the session to end
fn predict(link: Link, max_timeouts: Option<usize>, steps: &[Step]) -> (&[Step], Ending) {
    let mut model = Model::new(link, max_timeouts);
    for (index, step) in steps.iter().enumerate() {
        if let Some(ended) = model.step(*step) {
            return (&steps[..=index], Ending::Ended(ended));
        }
    }
    (steps, Ending::Running)
}

/// A frame carrying `pdu` to or from `unit`; RTU frames have no transaction id
fn frame(link: Link, tx_id: u16, unit: u8, pdu: &[u8]) -> Vec<u8> {
    match link {
        Link::Tcp => mbap(tx_id, unit, pdu),
        #[cfg(feature = "serial")]
        Link::Rtu => rtu(unit, pdu),
    }
}

/// A frame from `unit` the client can't parse, and the error it reports for it
fn garbage(link: Link, tx_id: u16, unit: u8) -> (Vec<u8>, FrameParseError) {
    match link {
        Link::Tcp => {
            let [high, low] = tx_id.to_be_bytes();
            let frame = vec![
                high, low, 0xCA, 0xFE, 0x00, 0x05, unit, 0x03, 0x02, 0x00, 0x00,
            ];
            (frame, FrameParseError::UnknownProtocolId(0xCAFE))
        }
        #[cfg(feature = "serial")]
        Link::Rtu => {
            let good = rtu(unit, &register_value(0));
            let Some((body, crc)) = good.split_last_chunk::<2>() else {
                panic!("an RTU frame ends with its CRC");
            };
            let crc = u16::from_le_bytes(*crc);
            let wrong = crc ^ 0xFF00;
            let frame = body.iter().copied().chain(wrong.to_le_bytes()).collect();
            (frame, FrameParseError::CrcValidationFailure(wrong, crc))
        }
    }
}

/// What the peer writes once it read the request with `tx_id` to `unit`, before its deadline
fn reply_bytes(link: Link, tx_id: u16, unit: u8, reply: Reply) -> Vec<u8> {
    let answer = |value| frame(link, tx_id, unit, &register_value(value));
    match reply {
        Reply::Answer(value) => answer(value),
        Reply::Exception(code, _) => frame(link, tx_id, unit, &[0x83, code]),
        Reply::WrongFunction => frame(link, tx_id, unit, &COILS_ON_OFF),
        Reply::OtherUnitThenAnswer(value) => {
            let other = unit.wrapping_add(1);
            [
                frame(link, tx_id, other, &register_value(!value)),
                answer(value),
            ]
            .concat()
        }
        Reply::StaleTxThenAnswer(value) => {
            let stale = tx_id.wrapping_sub(1);
            [
                frame(link, stale, unit, &register_value(!value)),
                answer(value),
            ]
            .concat()
        }
        Reply::Silence | Reply::LateAfterTimeout(_) => vec![],
        Reply::Garbage => garbage(link, tx_id, unit).0,
    }
}

/// Make a request of `unit` that the peer replies to with `reply`, check what the client wrote
/// and the result against the model, and return how the session ends with it, if it does
async fn request(
    link: Link,
    model: &mut Model,
    channel: &Channel,
    session: &mut Session,
    unit: u8,
    reply: Reply,
) -> Option<SessionError> {
    let (tx_id, expected, ends) = model.request(unit, reply);
    let started = Instant::now();
    let request = {
        let request = read_register(channel, unit);
        tokio::spawn(async move { (request.await.unwrap(), Instant::now()) })
    };
    session
        .expect(&frame(link, tx_id, unit, &READ_REGISTER))
        .await;
    session.send(&reply_bytes(link, tx_id, unit, reply)).await;

    // a result is due at once, or at the deadline
    let wait = if expected.elapsed == TIMEOUT {
        PAST_TIMEOUT
    } else {
        TICK
    };
    tokio::time::sleep(wait).await;
    assert!(request.is_finished(), "no result after {wait:?}");
    let (result, reported) = request.await.unwrap();
    let outcome = Outcome {
        result,
        elapsed: reported.duration_since(started),
    };
    assert_eq!(outcome, expected);

    // a late answer reaches only a session that is still running
    if let (Reply::LateAfterTimeout(value), None) = (reply, &ends) {
        session
            .send(&frame(link, tx_id, unit, &register_value(value)))
            .await;
        tokio::time::sleep(TICK).await;
    }
    ends
}

/// Change a setting through the channel, and return how the session ends with it, if it does
async fn change(model: &Model, channel: &Channel, change: Change) -> Option<SessionError> {
    match change {
        Change::DecodeLevel(level) => channel.set_decode_level(level).await,
        Change::Enable => channel.enable().await,
        Change::Disable => channel.disable().await,
    }
    .unwrap();
    tokio::time::sleep(TICK).await;
    model.setting(change)
}

/// How the session ended, after checking it wrote nothing more
async fn finish(mut session: Session) -> SessionError {
    let (ended, _) = (&mut session.task).await.unwrap();
    // the session dropped its end of the pipe, so the peer reads what is left, then the end
    let mut rest = vec![];
    session.peer.read_to_end(&mut rest).await.unwrap();
    assert_eq!(rest, []);
    ended
}

/// Run a session of `steps`, checking each against the model, and return how it ended
async fn execute(link: Link, max_timeouts: Option<usize>, steps: &[Step]) -> Ending {
    let setup = match link {
        Link::Tcp => Setup::tcp(),
        #[cfg(feature = "serial")]
        Link::Rtu => Setup::rtu(),
    };
    let (channel, mut client_loop) = setup.max_timeouts(max_timeouts.unwrap_or(0)).build();
    // a session runs once the channel is enabled; a setting received while disabled ends it
    client_loop.change_setting(Setting::Enable);
    let mut session = Session::run(client_loop, PIPE);
    let mut model = Model::new(link, max_timeouts);

    for step in steps {
        let ends = match *step {
            Step::Request { unit, reply } => {
                request(link, &mut model, &channel, &mut session, unit, reply).await
            }
            Step::Setting(setting) => change(&model, &channel, setting).await,
        };
        match ends {
            Some(expected) => {
                assert!(
                    session.task.is_finished(),
                    "still running, not {expected:?}"
                );
                let ended = finish(session).await;
                assert_eq!(ended, expected);
                return Ending::Ended(ended);
            }
            None => assert!(!session.task.is_finished(), "the session ended"),
        }
    }

    // the session is still running: it answers a request, then ends once the channel is dropped
    let ends = request(
        link,
        &mut model,
        &channel,
        &mut session,
        1,
        Reply::Answer(0xABCD),
    )
    .await;
    assert_eq!(ends, None);
    drop(channel);
    assert_eq!(finish(session).await, SessionError::Shutdown);
    Ending::Running
}

/// Run a session of `steps` on its own paused runtime, checking each against the model, and
/// return how it ended
fn check(link: Link, max_timeouts: Option<usize>, steps: &[Step]) -> Ending {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap();
    let mut ending = None;
    runtime.block_on(watchdog(async {
        ending = Some(execute(link, max_timeouts, steps).await);
    }));
    ending.unwrap()
}

/// Exception codes, and the exceptions the client reports for them
const EXCEPTIONS: [(u8, ExceptionCode); 4] = [
    (0x02, ExceptionCode::IllegalDataAddress),
    (0x04, ExceptionCode::ServerDeviceFailure),
    (0x0B, ExceptionCode::GatewayTargetDeviceFailedToRespond),
    (0x7F, ExceptionCode::Unknown(0x7F)),
];

fn unit() -> impl Strategy<Value = u8> {
    prop::sample::select(vec![0u8, 1, 2, 255])
}

/// A reply that lets the request time out
fn timeout() -> impl Strategy<Value = Reply> {
    prop_oneof![
        Just(Reply::Silence),
        any::<u16>().prop_map(Reply::LateAfterTimeout),
    ]
}

fn reply(link: Link) -> BoxedStrategy<Reply> {
    let reply = prop_oneof![
        4 => any::<u16>().prop_map(Reply::Answer),
        2 => prop::sample::select(EXCEPTIONS.to_vec())
            .prop_map(|(code, exception)| Reply::Exception(code, exception)),
        1 => Just(Reply::WrongFunction),
        2 => any::<u16>().prop_map(Reply::OtherUnitThenAnswer),
        4 => timeout(),
        1 => Just(Reply::Garbage),
    ];
    match link {
        Link::Tcp => prop_oneof![
            14 => reply,
            2 => any::<u16>().prop_map(Reply::StaleTxThenAnswer),
        ]
        .boxed(),
        #[cfg(feature = "serial")]
        Link::Rtu => reply.boxed(),
    }
}

fn step(link: Link) -> impl Strategy<Value = Step> {
    let request = (unit(), reply(link)).prop_map(|(unit, reply)| Step::Request { unit, reply });
    let decode = DecodeLevel::new(
        AppDecodeLevel::DataValues,
        FrameDecodeLevel::Payload,
        PhysDecodeLevel::Data,
    );
    let setting = prop_oneof![
        Just(Change::DecodeLevel(DecodeLevel::nothing())),
        Just(Change::DecodeLevel(decode)),
        Just(Change::Enable),
        Just(Change::Disable),
    ];
    prop_oneof![
        8 => request,
        1 => setting.prop_map(Step::Setting),
    ]
}

/// 1 to 12 steps, with runs of timeouts as long as the largest maximum, which few runs of single
/// steps reach
fn steps(link: Link) -> impl Strategy<Value = Vec<Step>> {
    let timeouts = prop::collection::vec(
        (unit(), timeout()).prop_map(|(unit, reply)| Step::Request { unit, reply }),
        1..=4,
    );
    let chunk = prop_oneof![
        4 => step(link).prop_map(|step| vec![step]),
        1 => timeouts,
    ];
    prop::collection::vec(chunk, 1..=12).prop_map(|chunks| {
        let mut steps = chunks.concat();
        steps.truncate(12);
        steps
    })
}

fn max_timeouts() -> impl Strategy<Value = Option<usize>> {
    prop::sample::select(vec![None, Some(1), Some(2), Some(3), Some(4)])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn tcp_sessions_match_the_model(max_timeouts in max_timeouts(), steps in steps(Link::Tcp)) {
        check(Link::Tcp, max_timeouts, &steps);
    }
}

#[cfg(feature = "serial")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn rtu_sessions_match_the_model(max_timeouts in max_timeouts(), steps in steps(Link::Rtu)) {
        check(Link::Rtu, max_timeouts, &steps);
    }
}

/// Check that cases drawn as the property tests draw them run every kind of step, and end in
/// every way, by the model
fn assert_generator_reaches_everything(link: Link, kinds: &[Kind]) {
    let mut runner = TestRunner::deterministic();
    let strategy = (max_timeouts(), steps(link));
    let mut seen = std::collections::BTreeSet::new();
    let mut endings = vec![];
    for _ in 0..128 {
        let (max_timeouts, steps) = strategy.new_tree(&mut runner).unwrap().current();
        // the session runs no step after the one that ends it
        let (run, ending) = predict(link, max_timeouts, &steps);
        seen.extend(run.iter().map(|step| step.kind()));
        endings.push(ending);
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), kinds);
    for ending in [
        Ending::Running,
        Ending::Ended(SessionError::MaxTimeouts(1)),
        Ending::Ended(SessionError::MaxTimeouts(2)),
        Ending::Ended(SessionError::MaxTimeouts(3)),
        Ending::Ended(SessionError::MaxTimeouts(4)),
        Ending::Ended(SessionError::BadFrame),
        Ending::Ended(SessionError::Disabled),
    ] {
        assert!(endings.contains(&ending), "no case ends {ending:?}");
    }
}

#[test]
fn the_tcp_generator_reaches_every_step_and_ending() {
    assert_generator_reaches_everything(
        Link::Tcp,
        &[
            Kind::Answer,
            Kind::Exception,
            Kind::WrongFunction,
            Kind::OtherUnitThenAnswer,
            Kind::StaleTxThenAnswer,
            Kind::Silence,
            Kind::LateAfterTimeout,
            Kind::Garbage,
            Kind::DecodeLevel,
            Kind::Enable,
            Kind::Disable,
        ],
    );
}

#[cfg(feature = "serial")]
#[test]
fn the_rtu_generator_reaches_every_step_but_a_stale_transaction_and_every_ending() {
    assert_generator_reaches_everything(
        Link::Rtu,
        &[
            Kind::Answer,
            Kind::Exception,
            Kind::WrongFunction,
            Kind::OtherUnitThenAnswer,
            Kind::Silence,
            Kind::LateAfterTimeout,
            Kind::Garbage,
            Kind::DecodeLevel,
            Kind::Enable,
            Kind::Disable,
        ],
    );
}

/// A read of [`REGISTER`] from unit 1 that times out
const SILENCE: Step = Step::Request {
    unit: 1,
    reply: Reply::Silence,
};

/// A read of [`REGISTER`] from unit 1 that is answered
const ANSWERED: Step = Step::Request {
    unit: 1,
    reply: Reply::Answer(0x1234),
};

#[test]
fn terminates_after_max_consecutive_timeouts() {
    assert_eq!(
        check(Link::Tcp, Some(3), &[SILENCE; 3]),
        Ending::Ended(SessionError::MaxTimeouts(3))
    );
}

#[test]
fn disabled_when_none_allows_unlimited_timeouts() {
    assert_eq!(check(Link::Tcp, None, &[SILENCE; 10]), Ending::Running);
}

#[test]
fn counter_resets_on_successful_request() {
    let steps = [SILENCE, SILENCE, ANSWERED, SILENCE, SILENCE];
    // only two consecutive timeouts, so the session is still running
    assert_eq!(check(Link::Tcp, Some(3), &steps), Ending::Running);
}

#[test]
fn counter_resets_on_an_exception() {
    let exception = Step::Request {
        unit: 1,
        reply: Reply::Exception(0x02, ExceptionCode::IllegalDataAddress),
    };
    // the exception breaks the run of timeouts, so the session is still running
    assert_eq!(
        check(Link::Tcp, Some(2), &[SILENCE, exception, SILENCE]),
        Ending::Running
    );
}

#[test]
fn disabling_the_channel_ends_the_session_and_other_settings_dont() {
    let decode = DecodeLevel::default().application(AppDecodeLevel::DataValues);
    let steps = [
        ANSWERED,
        Step::Setting(Change::DecodeLevel(decode)),
        Step::Setting(Change::Enable),
        ANSWERED,
        Step::Setting(Change::Disable),
    ];
    assert_eq!(
        check(Link::Tcp, None, &steps),
        Ending::Ended(SessionError::Disabled)
    );
}

#[test]
fn a_late_answer_while_idle_is_ignored() {
    let late = Step::Request {
        unit: 1,
        reply: Reply::LateAfterTimeout(0x1234),
    };
    // the next request is answered with its own transaction id
    assert_eq!(check(Link::Tcp, None, &[late]), Ending::Running);
}
