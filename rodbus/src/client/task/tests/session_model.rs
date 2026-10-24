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
use crate::common::test_support::{frame, Link};
use crate::decode::{FrameDecodeLevel, PhysDecodeLevel};
use crate::error::{AduParseError, FrameParseError};
use crate::exception::ExceptionCode;

/// Every wait is a multiple of this, and [`TIMEOUT`] isn't
const TICK: Duration = Duration::from_millis(7);

/// The first multiple of [`TICK`] past [`TIMEOUT`]
const PAST_TIMEOUT: Duration = Duration::from_millis(7 * 143);

/// The address of the register every request reads
const REGISTER: u16 = 0x10;

/// A transaction id next to a request's
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Adjacent {
    /// the previous request's, which a late answer to it carries
    Previous,
    Next,
}

impl Adjacent {
    /// The transaction id next to `tx_id`
    fn of(self, tx_id: u16) -> u16 {
        match self {
            Adjacent::Previous => tx_id.wrapping_sub(1),
            Adjacent::Next => tx_id.wrapping_add(1),
        }
    }
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
    /// a response with another value from unit `from`, which isn't the request's, then the
    /// answer with `value`
    OtherUnitThenAnswer { from: u8, value: u16 },
    /// a response with another value and a transaction id next to the request's, then the answer
    /// with `value`; TCP only, since RTU frames carry no transaction id
    OtherTxThenAnswer { tx: Adjacent, value: u16 },
    /// nothing, so the request times out
    Silence,
    /// nothing until the request timed out, then the answer, while no request is outstanding
    LateAfterTimeout(u16),
    /// a frame the client can't parse: a non-Modbus protocol id over TCP, which ends the session,
    /// or a wrong CRC over RTU, whose bytes the client drops, so the request times out
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
    OtherTxThenAnswer(Adjacent),
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
                Reply::OtherUnitThenAnswer { .. } => Kind::OtherUnitThenAnswer,
                Reply::OtherTxThenAnswer { tx, .. } => Kind::OtherTxThenAnswer(tx),
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
    Ended(SessionEnd),
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
    max_timeouts: Option<NonZeroUsize>,
    /// the transaction id of the next request, whatever became of the earlier ones
    next_tx_id: u16,
    /// response timeouts since the session started or since a request ended otherwise
    timeouts: usize,
}

impl Model {
    fn new(link: Link, max_timeouts: Option<NonZeroUsize>) -> Self {
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
    fn request(&mut self, unit: u8, reply: Reply) -> (u16, Outcome, Option<SessionEnd>) {
        let tx_id = self.next_tx_id;
        self.next_tx_id = tx_id.wrapping_add(1);
        let result = match reply {
            Reply::Garbage if self.link != Link::Tcp => Err(RequestError::ResponseTimeout),
            Reply::Garbage => {
                let (_, error) = garbage(self.link, tx_id, unit);
                let outcome = Outcome {
                    result: Err(RequestError::BadFrame(error)),
                    elapsed: Duration::ZERO,
                };
                return (
                    tx_id,
                    outcome,
                    Some(SessionEnd::Failed(SessionFailure::BadFrame)),
                );
            }
            _ if self.is_broadcast(unit) => Err(RequestError::ResponseTimeout),
            Reply::Silence | Reply::LateAfterTimeout(_) => Err(RequestError::ResponseTimeout),
            Reply::Answer(value)
            | Reply::OtherUnitThenAnswer { value, .. }
            | Reply::OtherTxThenAnswer { value, .. } => Ok(vec![Indexed::new(REGISTER, value)]),
            Reply::Exception(_, exception) => Err(RequestError::Exception(exception)),
            Reply::WrongFunction => Err(RequestError::BadResponse(
                AduParseError::UnknownResponseFunction(0x01, 0x03, 0x83),
            )),
        };
        if result == Err(RequestError::ResponseTimeout) {
            self.timeouts = self.timeouts.saturating_add(1);
            let ends = self
                .max_timeouts
                .filter(|max| self.timeouts >= max.get())
                .map(|max| SessionEnd::Failed(SessionFailure::MaxTimeouts(max.get())));
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
    fn setting(&self, change: Change) -> Option<SessionEnd> {
        match change {
            Change::DecodeLevel(_) | Change::Enable => None,
            Change::Disable => Some(SessionEnd::Stopped(StateChange::Disable)),
        }
    }

    /// How the session ends with a step, if it does
    fn step(&mut self, step: Step) -> Option<SessionEnd> {
        match step {
            Step::Request { unit, reply } => self.request(unit, reply).2,
            Step::Setting(change) => self.setting(change),
        }
    }
}

/// The steps a session of `steps` runs, through the one that ends it, and how the model expects
/// the session to end
fn predict(link: Link, max_timeouts: Option<NonZeroUsize>, steps: &[Step]) -> (&[Step], Ending) {
    let mut model = Model::new(link, max_timeouts);
    for (index, step) in steps.iter().enumerate() {
        if let Some(ended) = model.step(*step) {
            return (&steps[..=index], Ending::Ended(ended));
        }
    }
    (steps, Ending::Running)
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
            let good = crate::serial::test_util::rtu(unit, &register_value(0));
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
        Reply::OtherUnitThenAnswer { from, value } => [
            frame(link, tx_id, from, &register_value(!value)),
            answer(value),
        ]
        .concat(),
        Reply::OtherTxThenAnswer { tx, value } => [
            frame(link, tx.of(tx_id), unit, &register_value(!value)),
            answer(value),
        ]
        .concat(),
        Reply::Silence | Reply::LateAfterTimeout(_) => vec![],
        Reply::Garbage => garbage(link, tx_id, unit).0,
    }
}

/// The bytes of a request step: what the client wrote, and what the peer wrote back before the
/// request's deadline
#[derive(Debug, PartialEq)]
struct Exchange {
    request: Vec<u8>,
    reply: Vec<u8>,
}

/// Make a request of `unit` that the peer replies to with `reply`, check what the client wrote
/// and the result against the model, and return the bytes exchanged and how the session ends
/// with it, if it does
async fn request(
    link: Link,
    model: &mut Model,
    channel: &Channel,
    session: &mut Session,
    unit: u8,
    reply: Reply,
) -> (Exchange, Option<SessionEnd>) {
    let (tx_id, expected, ends) = model.request(unit, reply);
    let started = Instant::now();
    let request = {
        let request = read_register(channel, unit);
        tokio::spawn(async move { (request.await.unwrap(), Instant::now()) })
    };
    let exchange = Exchange {
        request: frame(link, tx_id, unit, &READ_REGISTER),
        reply: reply_bytes(link, tx_id, unit, reply),
    };
    session.expect(&exchange.request).await;
    session.send(&exchange.reply).await;

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
    (exchange, ends)
}

/// Change a setting through the channel, and return how the session ends with it, if it does
async fn change(model: &Model, channel: &Channel, change: Change) -> Option<SessionEnd> {
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
async fn finish(mut session: Session) -> SessionEnd {
    let (ended, _) = (&mut session.task).await.unwrap();
    // the session dropped its end of the pipe, so the peer reads what is left, then the end
    let mut rest = vec![];
    session.peer.read_to_end(&mut rest).await.unwrap();
    assert_eq!(rest, []);
    ended
}

/// Run a session of `steps`, checking each against the model, and return how it ended and the
/// bytes of each request step it ran
async fn execute(
    link: Link,
    max_timeouts: Option<NonZeroUsize>,
    steps: &[Step],
) -> (Ending, Vec<Exchange>) {
    let setup = match link {
        Link::Tcp => Setup::tcp(),
        #[cfg(feature = "serial")]
        Link::Rtu => Setup::rtu(),
    };
    let (channel, mut client_loop) = setup.max_timeouts(max_timeouts).build();
    // a session runs once the channel is enabled; a setting received while disabled ends it
    client_loop.change_setting(Setting::Enable);
    let mut session = Session::run(client_loop, PIPE);
    let mut model = Model::new(link, max_timeouts);
    let mut exchanges = vec![];

    for step in steps {
        let ends = match *step {
            Step::Request { unit, reply } => {
                let (exchange, ends) =
                    request(link, &mut model, &channel, &mut session, unit, reply).await;
                exchanges.push(exchange);
                ends
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
                return (Ending::Ended(ended), exchanges);
            }
            None => assert!(!session.task.is_finished(), "the session ended"),
        }
    }

    // the session is still running: it answers a request, then ends once the channel is dropped
    let (_, ends) = request(
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
    assert_eq!(
        finish(session).await,
        SessionEnd::Stopped(StateChange::Shutdown)
    );
    (Ending::Running, exchanges)
}

/// Run a session of `steps` on its own paused runtime, checking each against the model, and
/// return how it ended
fn check(link: Link, max_timeouts: Option<NonZeroUsize>, steps: &[Step]) -> Ending {
    check_exchanges(link, max_timeouts, steps).0
}

/// [`check`], also returning the bytes of each request step the session ran
fn check_exchanges(
    link: Link,
    max_timeouts: Option<NonZeroUsize>,
    steps: &[Step],
) -> (Ending, Vec<Exchange>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap();
    let mut ran = None;
    runtime.block_on(watchdog(async {
        ran = Some(execute(link, max_timeouts, steps).await);
    }));
    ran.unwrap()
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

/// What the peer does once it read a request to `unit`
fn reply(link: Link, unit: u8) -> BoxedStrategy<Reply> {
    let reply = prop_oneof![
        4 => any::<u16>().prop_map(Reply::Answer),
        2 => prop::sample::select(EXCEPTIONS.to_vec())
            .prop_map(|(code, exception)| Reply::Exception(code, exception)),
        1 => Just(Reply::WrongFunction),
        2 => (1..=u8::MAX, any::<u16>()).prop_map(move |(offset, value)| {
            Reply::OtherUnitThenAnswer {
                from: unit.wrapping_add(offset),
                value,
            }
        }),
        4 => timeout(),
        1 => Just(Reply::Garbage),
    ];
    match link {
        Link::Tcp => prop_oneof![
            14 => reply,
            2 => (
                prop::sample::select(vec![Adjacent::Previous, Adjacent::Next]),
                any::<u16>(),
            )
                .prop_map(|(tx, value)| Reply::OtherTxThenAnswer { tx, value }),
        ]
        .boxed(),
        #[cfg(feature = "serial")]
        Link::Rtu => reply.boxed(),
    }
}

fn step(link: Link) -> impl Strategy<Value = Step> {
    let request = unit().prop_flat_map(move |unit| {
        reply(link, unit).prop_map(move |reply| Step::Request { unit, reply })
    });
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

fn max_timeouts() -> impl Strategy<Value = Option<NonZeroUsize>> {
    prop::sample::select(vec![
        None,
        NonZeroUsize::new(1),
        NonZeroUsize::new(2),
        NonZeroUsize::new(3),
        NonZeroUsize::new(4),
    ])
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
fn assert_generator_reaches_everything(link: Link, kinds: &[Kind], endings_reached: &[Ending]) {
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
    for ending in endings_reached {
        assert!(endings.contains(ending), "no case ends {ending:?}");
    }
}

/// The endings every link reaches, and a bad frame if `bad_frame`
fn endings(bad_frame: bool) -> Vec<Ending> {
    let mut endings = vec![
        Ending::Running,
        Ending::Ended(SessionEnd::Failed(SessionFailure::MaxTimeouts(1))),
        Ending::Ended(SessionEnd::Failed(SessionFailure::MaxTimeouts(2))),
        Ending::Ended(SessionEnd::Failed(SessionFailure::MaxTimeouts(3))),
        Ending::Ended(SessionEnd::Failed(SessionFailure::MaxTimeouts(4))),
        Ending::Ended(SessionEnd::Stopped(StateChange::Disable)),
    ];
    if bad_frame {
        endings.push(Ending::Ended(SessionEnd::Failed(SessionFailure::BadFrame)));
    }
    endings
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
            Kind::OtherTxThenAnswer(Adjacent::Previous),
            Kind::OtherTxThenAnswer(Adjacent::Next),
            Kind::Silence,
            Kind::LateAfterTimeout,
            Kind::Garbage,
            Kind::DecodeLevel,
            Kind::Enable,
            Kind::Disable,
        ],
        &endings(true),
    );
}

/// RTU sessions drop the bytes of a bad frame rather than end
#[cfg(feature = "serial")]
#[test]
fn the_rtu_generator_reaches_every_step_but_another_transaction_and_every_ending_but_a_bad_frame() {
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
        &endings(false),
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
        check(Link::Tcp, NonZeroUsize::new(3), &[SILENCE; 3]),
        Ending::Ended(SessionEnd::Failed(SessionFailure::MaxTimeouts(3)))
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
    assert_eq!(
        check(Link::Tcp, NonZeroUsize::new(3), &steps),
        Ending::Running
    );
}

#[test]
fn counter_resets_on_an_exception() {
    let exception = Step::Request {
        unit: 1,
        reply: Reply::Exception(0x02, ExceptionCode::IllegalDataAddress),
    };
    // the exception breaks the run of timeouts, so the session is still running
    assert_eq!(
        check(
            Link::Tcp,
            NonZeroUsize::new(2),
            &[SILENCE, exception, SILENCE]
        ),
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
        Ending::Ended(SessionEnd::Stopped(StateChange::Disable))
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

/// The requests a session wrote, in order
fn requests(exchanges: Vec<Exchange>) -> Vec<Vec<u8>> {
    exchanges
        .into_iter()
        .map(|exchange| exchange.request)
        .collect()
}

#[test]
fn tcp_requests_carry_the_unit_and_successive_transaction_ids() {
    let steps = [0, 0xFF, 1].map(|unit| Step::Request {
        unit,
        reply: Reply::Answer(0x1234),
    });
    // the model checks that each request reports 0x1234 from 0x10
    let (ending, exchanges) = check_exchanges(Link::Tcp, None, &steps);
    assert_eq!(ending, Ending::Running);
    assert_eq!(
        requests(exchanges),
        [
            mbap(0, 0, &READ_REGISTER),
            mbap(1, 0xFF, &READ_REGISTER),
            mbap(2, 1, &READ_REGISTER),
        ]
    );
}

#[cfg(feature = "serial")]
#[test]
fn rtu_requests_carry_the_unit_address() {
    use crate::serial::test_util::rtu;

    let steps = [1, 0xFF].map(|unit| Step::Request {
        unit,
        reply: Reply::Answer(0x1234),
    });
    // the model checks that each request reports 0x1234 from 0x10
    let (ending, exchanges) = check_exchanges(Link::Rtu, None, &steps);
    assert_eq!(ending, Ending::Running);
    assert_eq!(
        requests(exchanges),
        [rtu(1, &READ_REGISTER), rtu(0xFF, &READ_REGISTER)]
    );
}

/// A request to unit 2 that unit 1 replies to first, then unit 2 with 0x2222
const FROM_UNIT_1_THEN_2: Step = Step::Request {
    unit: 2,
    reply: Reply::OtherUnitThenAnswer {
        from: 1,
        value: 0x2222,
    },
};

#[test]
fn a_tcp_response_from_another_unit_is_skipped() {
    // the model checks that the request reports 0x2222 at once
    let (ending, exchanges) = check_exchanges(Link::Tcp, None, &[FROM_UNIT_1_THEN_2]);
    assert_eq!(ending, Ending::Running);
    // the same transaction id, from unit 1 and then from unit 2
    assert_eq!(
        exchanges,
        [Exchange {
            request: mbap(0, 2, &READ_REGISTER),
            reply: [
                mbap(0, 1, &register_value(0xDDDD)),
                mbap(0, 2, &register_value(0x2222)),
            ]
            .concat(),
        }]
    );
}

#[cfg(feature = "serial")]
#[test]
fn an_rtu_response_from_another_unit_is_skipped() {
    use crate::serial::test_util::rtu;

    // the model checks that the request reports 0x2222 at once
    let (ending, exchanges) = check_exchanges(Link::Rtu, None, &[FROM_UNIT_1_THEN_2]);
    assert_eq!(ending, Ending::Running);
    // a reply from unit 1, then unit 2's
    assert_eq!(
        exchanges,
        [Exchange {
            request: rtu(2, &READ_REGISTER),
            reply: [
                rtu(1, &register_value(0xDDDD)),
                rtu(2, &register_value(0x2222)),
            ]
            .concat(),
        }]
    );
}

#[test]
fn a_tcp_frame_answers_only_with_the_same_transaction_and_unit() {
    // seven answered requests, so the next is written with transaction id 7
    let earlier = [ANSWERED; 7];
    // a frame to unit 2 with transaction id 8, and one to unit 1 with transaction id 7
    let skipped = [
        (
            Reply::OtherTxThenAnswer {
                tx: Adjacent::Next,
                value: 0x2222,
            },
            mbap(8, 2, &register_value(0xDDDD)),
        ),
        (
            Reply::OtherUnitThenAnswer {
                from: 1,
                value: 0x2222,
            },
            mbap(7, 1, &register_value(0xDDDD)),
        ),
    ];
    for (reply, skipped) in skipped {
        let steps: Vec<_> = earlier
            .into_iter()
            .chain([Step::Request { unit: 2, reply }])
            .collect();
        // the model checks that the request reports 0x2222 at once
        let (ending, exchanges) = check_exchanges(Link::Tcp, None, &steps);
        assert_eq!(ending, Ending::Running);
        assert_eq!(
            exchanges.last(),
            Some(&Exchange {
                request: mbap(7, 2, &READ_REGISTER),
                reply: [skipped, mbap(7, 2, &register_value(0x2222))].concat(),
            })
        );
    }
}
