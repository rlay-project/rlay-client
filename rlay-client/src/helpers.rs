use futures::prelude::*;
use futures01 as futures_01;
use tokio_futures3 as tokio_02;

struct CompatExecutor;

impl tokio_executor_01::Executor for CompatExecutor {
    fn spawn(
        &mut self,
        future: Box<dyn futures_01::Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), tokio_executor_01::SpawnError> {
        tokio_02::spawn(futures::compat::Compat01As03::new(future).map(|_| ()));
        Ok(())
    }
}

pub trait CompatBlockOn {
    fn block_on_with_01<F: Future>(&mut self, fut: F) -> F::Output;
}

impl CompatBlockOn for tokio_02::runtime::Runtime {
    fn block_on_with_01<F: Future>(&mut self, fut: F) -> F::Output {
        let mut executor = CompatExecutor;
        let mut enter = tokio_executor_01::enter().unwrap();

        tokio_executor_01::with_default(&mut executor, &mut enter, |_| self.block_on(fut))
    }
}

impl CompatBlockOn for futures::executor::ThreadPool {
    fn block_on_with_01<F: Future>(&mut self, fut: F) -> F::Output {
        let mut executor = CompatExecutor;
        let mut enter = tokio_executor_01::enter().unwrap();

        tokio_executor_01::with_default(&mut executor, &mut enter, |_| self.run(fut))
    }
}
