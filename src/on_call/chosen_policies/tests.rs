use super::*;

fn policy(id: &str) -> EscalationPolicyId {
    EscalationPolicyId::try_new(id).unwrap()
}

#[test]
fn policies_are_read_with_or_without_whitespace_around_them() {
    let spaced = ChosenPolicies::from(" PANZZEQ , PT20YPA ");
    let dense = ChosenPolicies::from("PANZZEQ,PT20YPA");

    assert_eq!(spaced, dense);
    assert_eq!(dense.0, [policy("PANZZEQ"), policy("PT20YPA")]);
}

#[test]
fn a_blank_string_and_blank_pieces_choose_no_policy() {
    assert_eq!(ChosenPolicies::from(""), ChosenPolicies::default());
    assert_eq!(ChosenPolicies::from("  "), ChosenPolicies::default());
    assert_eq!(
        ChosenPolicies::from("PANZZEQ,, ,PT20YPA,"),
        ChosenPolicies::from("PANZZEQ,PT20YPA")
    );
}

#[test]
fn only_the_chosen_policies_are_included() {
    let chosen = ChosenPolicies::from("PANZZEQ, PT20YPA");

    assert!(chosen.include(&policy("PANZZEQ")));
    assert!(chosen.include(&policy("PT20YPA")));
    assert!(!chosen.include(&policy("PT20YP")));
    assert!(!chosen.include(&policy("panzzeq")));
}

#[test]
fn with_no_policy_chosen_every_policy_is_included() {
    assert!(ChosenPolicies::default().include(&policy("PANZZEQ")));
}
