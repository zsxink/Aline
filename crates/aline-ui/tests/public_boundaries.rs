//! 宿主身份信封的无窗口外部消费者。

use aline_buffer::{DocumentRevision, TextBudget, TextSnapshot};
use aline_ui::{DocumentId, DraftGeneration, IdentityError, InputIdentity, WorkingVersion};

#[test]
fn input_identity_can_be_carried_by_future_host_tasks() {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<InputIdentity>();
    require_send_sync::<DocumentId>();
}

#[test]
fn input_identity_rejects_wrong_document_or_working_version()
-> Result<(), Box<dyn std::error::Error>> {
    let source = TextSnapshot::new(String::from("正文"), TextBudget::new(64))?;
    let document = DocumentId::new();
    let version = WorkingVersion::new(DocumentRevision::initial(), DraftGeneration::initial());
    let input = InputIdentity::new(document.clone(), version, &source)?;
    assert!(input.validate(&input.clone()).is_ok());
    let wrong_document = InputIdentity::new(DocumentId::new(), version, &source)?;
    assert!(matches!(
        input.validate(&wrong_document),
        Err(IdentityError::WrongDocument)
    ));
    let later = InputIdentity::new(document, version.next_draft()?, &source)?;
    assert!(matches!(
        input.validate(&later),
        Err(IdentityError::StaleWorkingVersion)
    ));
    assert!(matches!(
        DraftGeneration::from_value(u64::MAX).next(),
        Err(IdentityError::GenerationExhausted)
    ));
    let same_text_new_snapshot = TextSnapshot::new(String::from("正文"), TextBudget::new(64))?;
    let wrong_snapshot = InputIdentity::new(
        input.document_id().clone(),
        version,
        &same_text_new_snapshot,
    )?;
    assert!(matches!(
        input.validate(&wrong_snapshot),
        Err(IdentityError::StaleSnapshot)
    ));
    Ok(())
}
