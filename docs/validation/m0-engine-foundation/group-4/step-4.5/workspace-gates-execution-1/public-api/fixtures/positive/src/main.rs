//! 公共边界隔离消费夹具。

fn main() -> Result<(), Box<dyn std::error::Error>> {

let snapshot = aline_buffer::TextSnapshot::new(String::from("甲😀"), aline_buffer::TextBudget::new(64))?;
let projection = aline_markdown::Projection::identity(snapshot.clone(), snapshot.src_range(0..7)?, aline_markdown::MappingBudget::new(64, 16))?;
let storage = aline_source::StorageSnapshot::new(vec![b'x'], aline_source::DiskVersion::initial(), aline_source::StorageBudget::new(8))?;

let source: aline_buffer::SrcOffset = projection.source_at(&projection.drawn_offset(3)?, aline_buffer::Affinity::Before)?.into_value();
let _drawn: aline_markdown::DrawnOffset = projection.drawn_at(&source, aline_buffer::Affinity::After)?.into_value();
let _utf16: aline_markdown::Utf16Offset = projection.utf16_from_source(&source, aline_buffer::Affinity::After)?.into_value();
let _stored: aline_source::StorageRange = storage.range(0..1)?;
let tuples: Vec<(std::ops::Range<usize>, String)> = vec![(0..0, String::from("!"))];
let batch = aline_buffer::CheckedBatch::new(&snapshot, tuples, aline_buffer::PatchBudget::new(4, 64, 64))?;
let _borrowed: &[(std::ops::Range<usize>, String)] = batch.patches();

Ok(())
}
