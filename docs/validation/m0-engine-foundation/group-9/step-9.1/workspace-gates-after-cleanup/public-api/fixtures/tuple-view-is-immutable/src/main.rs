//! 公共边界隔离消费夹具。

fn main() -> Result<(), Box<dyn std::error::Error>> {

let snapshot = aline_buffer::TextSnapshot::new(String::from("甲😀"), aline_buffer::TextBudget::new(64))?;
let projection = aline_markdown::Projection::identity(snapshot.clone(), snapshot.src_range(0..7)?, aline_markdown::MappingBudget::new(64, 16))?;
let storage = aline_source::StorageSnapshot::new(vec![b'x'], aline_source::DiskVersion::initial(), aline_source::StorageBudget::new(8))?;
let batch = aline_buffer::CheckedBatch::new(&snapshot, vec![(0..0, String::new())], aline_buffer::PatchBudget::new(4, 64, 64))?; batch.patches()[0].1.push_str("!");
Ok(())
}
