use markinim::filter::{ContentRules, is_message_ok};

fn rules(sfw: bool, links: bool, usernames: bool) -> ContentRules {
    ContentRules::new(sfw, links, usernames)
}

#[test]
fn blank_messages_rejected() {
    assert!(!is_message_ok(&rules(false, false, false), ""));
    assert!(!is_message_ok(&rules(false, false, false), "   \n\t "));
}

#[test]
fn sfw_filter_matches_substrings_case_insensitively() {
    let r = rules(true, false, false);
    assert!(!is_message_ok(&r, "that is such shit"));
    assert!(!is_message_ok(&r, "classic ASSlover move"));
    // no anchors in the pattern: substrings inside words match too, exactly
    // like the Nim regex
    assert!(!is_message_ok(&r, "i am an assistant"));
    assert!(is_message_ok(&r, "a perfectly fine sentence"));
}

#[test]
fn link_filter() {
    let r = rules(false, true, false);
    assert!(!is_message_ok(&r, "visit https://example.com/x now"));
    assert!(!is_message_ok(&r, "go to www.example.com ok?"));
    assert!(is_message_ok(&r, "no links here, just words"));
}

#[test]
fn username_filter_uses_lookahead_rules() {
    let r = rules(false, false, true);
    assert!(!is_message_ok(&r, "ping @somebody about it"));
    // too short / trailing underscore / starts with underscore: no match
    assert!(is_message_ok(&r, "ping @abc about it"));
    assert!(is_message_ok(&r, "email me a@b not a telegram username"));
    assert!(is_message_ok(&r, "plain words"));
}

#[test]
fn disabled_filters_pass_everything() {
    let r = rules(false, false, false);
    assert!(is_message_ok(&r, "shit https://example.com @somebody"));
}
