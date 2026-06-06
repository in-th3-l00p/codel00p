use codel00p_harness::crate_name;

#[test]
fn exposes_harness_crate_identity() {
    assert_eq!(crate_name(), "codel00p-harness");
}
