use codel00p_harness::IterationBudget;

#[test]
fn iteration_budget_consumes_until_limit() {
    let budget = IterationBudget::new(2);

    assert!(budget.consume());
    assert!(budget.consume());
    assert!(!budget.consume());
    assert_eq!(budget.used(), 2);
    assert_eq!(budget.remaining(), 0);
}

#[test]
fn iteration_budget_can_refund_work() {
    let budget = IterationBudget::new(2);

    assert!(budget.consume());
    budget.refund();

    assert_eq!(budget.used(), 0);
    assert_eq!(budget.remaining(), 2);
    assert!(budget.consume());
    assert!(budget.consume());
}
