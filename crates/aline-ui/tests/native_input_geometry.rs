//! W5 3.3 的公开边界契约：输入几何依赖的身份/投影代次在版本推进、模式切换
//! 与解析配置变化后必须拒收旧帧，且拒收零副作用；纯选区移动不推进身份。
//!
//! 真实候选矩形来自 GPUI 文本布局：`DocumentInput::bounds_for_range` 与
//! `character_index_for_point` 持有 crate 私有的 `PreparedLayout`、视口、缩放
//! 因子与投影 id，crate 外无法构造 `Window`。这些坐标与失效守卫由
//! `native_window/editable_input/tests.rs`、`native_window/readonly_hit.rs`
//! 的窗口内测试覆盖；真实 macOS 长文虚拟列表边缘、DPI/字体代次、候选窗
//! 跟随插入点仍需真实窗口取证，本目标不声称这些平台行为已通过。多屏缺设备
//! 保持未测。

mod common;

use aline_buffer::SelectionSpec;
use aline_ui::{DocCore, DocError, EditorMode};
use common::{capture, limits};

fn document(text: &str, at: usize) -> Result<DocCore, DocError> {
    DocCore::new(text, SelectionSpec::new(at, at), limits())
}

#[test]
fn geometry_identity_is_rejected_after_commit_without_side_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("abc\n", 1)?;
    let identity = doc.current_input_identity()?;
    doc.type_text(1, "中", 0)?;
    doc.commit_open_block(1)?;

    let committed = capture(&mut doc)?;
    assert!(matches!(
        doc.accept_derived_result(&identity),
        Err(DocError::StaleResult)
    ));
    assert_eq!(
        capture(&mut doc)?,
        committed,
        "旧几何结果被拒收时不得改变文档"
    );

    // 版本推进后的新身份必须可用：布局重新测量后按同一协议继续消费。
    let current = doc.current_input_identity()?;
    assert_ne!(identity, current, "提交必须让旧帧身份失效");
    doc.accept_derived_result(&current)?;
    Ok(())
}

#[test]
fn geometry_identity_is_rejected_after_mode_switch_and_parse_config_change()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("abc\n", 1)?;
    let visual_identity = doc.current_input_identity()?;
    doc.switch_mode(EditorMode::Source, 1)?;

    let switched = capture(&mut doc)?;
    assert!(matches!(
        doc.accept_derived_result(&visual_identity),
        Err(DocError::StaleResult)
    ));
    assert_eq!(capture(&mut doc)?, switched);

    let source_identity = doc.current_input_identity()?;
    doc.advance_parse_config()?;
    let advanced = capture(&mut doc)?;
    assert!(matches!(
        doc.accept_derived_result(&source_identity),
        Err(DocError::StaleResult)
    ));
    assert_eq!(capture(&mut doc)?, advanced);

    let refreshed = doc.current_input_identity()?;
    doc.accept_derived_result(&refreshed)?;
    Ok(())
}

#[test]
fn caret_move_within_the_open_block_keeps_the_geometry_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("abc\n", 1)?;
    doc.leave_block(0)?;
    doc.open_block(0..3, SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;

    // 方向键/指针移动只改选区，不推进版本、快照或请求代次；同一帧仍可消费。
    doc.set_selection(SelectionSpec::new(3, 1))?;
    assert_eq!(doc.current_selection(), SelectionSpec::new(3, 1));
    doc.accept_derived_result(&identity)?;
    assert_eq!(doc.undo_depth(), 0, "移动选区不得产生历史");
    Ok(())
}

#[test]
fn stale_identity_selection_is_rejected_without_state_change()
-> Result<(), Box<dyn std::error::Error>> {
    use aline_buffer::SrcSelection;

    let mut doc = document("abc\n", 1)?;
    let identity = doc.current_input_identity()?;
    doc.type_text(1, "中", 0)?;
    doc.commit_open_block(1)?;

    // 选区按提交后的当前快照构造；旧身份必须仍被拒收，而不是按新投影解释。
    let current = doc.current_snapshot(common::TEXT)?;
    let selection =
        SrcSelection::new(current.text().src_offset(0)?, current.text().src_offset(1)?)?;
    let committed = capture(&mut doc)?;
    assert!(doc.select(&identity, &selection).is_err());
    assert_eq!(capture(&mut doc)?, committed);

    let refreshed = doc.current_input_identity()?;
    doc.select(&refreshed, &selection)?;
    assert_eq!(doc.current_selection(), SelectionSpec::new(0, 1));
    Ok(())
}
