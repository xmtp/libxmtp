use futures::Future;
use tokio::{runtime::Handle, task};

pub fn await_helper<F: Future + Send>(future: F) -> F::Output
where
    <F as Future>::Output: Send,
{
    task::block_in_place(move || Handle::current().block_on(future))
}
