//! Announcements read from the docs' example answers.

use super::*;
use crate::test_support::{Recorded, block_on};

const LIST: &str = include_str!("../../../fixtures/aster/account_announcements.json");
const ONE: &str = include_str!("../../../fixtures/aster/account_announcement.json");

#[test]
fn announcements_read_in_each_language() {
    let transport = Recorded::answering(LIST);
    let page = block_on(announcements(&transport, Some(1), Some(20))).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.signed), ("/fapi/v3/announcement/direct", true));
    assert_eq!(call.fields, vec![("page", "1".to_owned()), ("size", "20".to_owned())]);
    assert_eq!((page.total, page.rows[0].id), (2, 1001));
    let transport = Recorded::answering(ONE);
    let one = block_on(announcement(&transport, 1001)).unwrap();
    assert_eq!(transport.only().fields, vec![("id", "1001".to_owned())]);
    assert_eq!(one.in_language("zh").unwrap().title, "平台维护公告");
    assert_eq!(one.in_language("fr").unwrap().language, "en");
    assert_eq!((one.category.as_str(), one.publish_time), ("MAINTENANCE", 1_750_000_000_000));
}
