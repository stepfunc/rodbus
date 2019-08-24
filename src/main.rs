

use tokio::sync::mpsc;
use tokio::sync::oneshot;

#[derive(Debug)]
struct Reply {
    pub result : usize,
}

impl Reply {
   fn new(result : usize) -> Self {
       Reply { result }
   }
}


struct Request {
    argument : usize,
    reply_to : tokio::sync::oneshot::Sender<Reply>
}

struct Client {
    sender: tokio::sync::mpsc::Sender<Request>
}


#[derive(Debug)]
enum Error {
    Tx,
    Rx
}

impl std::convert::From<tokio::sync::oneshot::error::RecvError> for Error {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Error::Rx
    }
}

impl std::convert::From<tokio::sync::mpsc::error::SendError> for Error {
    fn from(_: tokio::sync::mpsc::error::SendError) -> Self {
        Error::Tx
    }
}



impl Client {
    async fn square(&mut self, x : usize) -> Result<Reply, Error> {
        let (tx, rx) = oneshot::channel::<Reply>();
        self.sender.send(Request{argument : x, reply_to :  tx}).await?;
        rx.await.map_err(|_| { Error::Rx } )
    }

    fn new(sender: tokio::sync::mpsc::Sender<Request>) -> Client {
        Client { sender }
    }
}


async fn server(mut rx : mpsc::Receiver<Request>) {

    while let Some(request) =  rx.recv().await {
         if let Err(_e) = request.reply_to.send(Reply::new( request.argument * request.argument)) {
             // TODO
         }
    }

}

fn main() {


    let (tx, rx) = mpsc::channel(10);

    let mut client = Client::new(tx);

    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(server(rx));

    println!("result is: {:?}", rt.block_on(client.square(4)).unwrap());
}
