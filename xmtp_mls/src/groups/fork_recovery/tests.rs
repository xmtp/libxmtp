use crate::tester;

#[xmtp_macro::test]
async fn basic_fork_recovery_worker() {
    tester!(alix1, fork_recovery);
}
