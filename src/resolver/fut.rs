use std::future::Future;

use tokio::sync::oneshot;

pub struct Fut<T> {
    recv: oneshot::Receiver<T>,
}

impl<T> Fut<T>
where
    T: Send + 'static,
{
    pub const fn new(recv: oneshot::Receiver<T>) -> Self {
        Self { recv }
    }

    pub fn wrap<E>(self, wrap: impl FnOnce(T) -> E + Send + Sync + 'static) -> Fut<E>
    where
        E: Send + 'static,
    {
        <Fut<E>>::spawn(async { wrap(self.wait().await.expect("resolver future shouldn't panic")) })
    }

    pub fn spawn(fut: impl Future<Output = T> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            let result = fut.await;
            let _ = tx.send(result);
        });
        Self { recv: rx }
    }

    pub fn try_resolve(&mut self) -> Option<T> {
        self.recv.try_recv().ok()
    }

    pub async fn wait(self) -> Option<T> {
        self.recv.await.ok()
    }
}
