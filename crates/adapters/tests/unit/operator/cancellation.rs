use super::*;
use std::{cell::Cell, rc::Rc};

#[tokio::test]
async fn cancellation_drops_pending_work_and_does_not_report_success() {
    struct Guard(Rc<Cell<bool>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let guard = Guard(dropped.clone());
    let work = async move {
        let _guard = guard;
        std::future::pending::<Result<(), Failure>>().await
    };
    let result = race(work, std::future::ready(())).await;
    assert_eq!(result.unwrap_err().exit_code(), 1);
    assert!(dropped.get());
}

#[tokio::test]
async fn completion_and_failure_are_preserved_without_cancellation() {
    assert_eq!(
        race(async { Ok(7) }, std::future::pending()).await.unwrap(),
        7
    );
    assert_eq!(
        race(
            async { Err::<(), _>(Failure::confirmation()) },
            std::future::pending()
        )
        .await
        .unwrap_err()
        .exit_code(),
        3
    );
}
