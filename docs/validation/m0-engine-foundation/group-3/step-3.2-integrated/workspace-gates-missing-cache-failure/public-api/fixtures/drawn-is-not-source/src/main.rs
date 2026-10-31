//! 公共边界隔离消费夹具。
fn need(_: aline_buffer::SrcOffset) {}
fn main() -> Result<(), Box<dyn std::error::Error>> {

let snapshot = aline_buffer::TextSnapshot::new(String::from("甲😀"), aline_buffer::TextBudget::new(64))?;
let projection = aline_markdown::Projection::identity(snapshot.clone(), snapshot.src_range(0..7)?, aline_markdown::MappingBudget::new(64, 16))?;
let storage = aline_source::StorageSnapshot::new(vec![b'x'], aline_source::DiskVersion::initial(), aline_source::StorageBudget::new(8))?;
need(projection.drawn_offset(0)?);
Ok(())
}
