use std::io::{self, Read, Seek, SeekFrom};

/// Represents a single MP4 box
pub struct Mp4Box {
    pub box_type: String,
    pub size: u64,
    pub offset: u64,
}

/// Reads 4 bytes and interprets them as a big-endian u32
fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut buf = [0; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

/// Reads a single MP4 box (not recursively)
fn read_box(reader: &mut (impl Read + Seek)) -> io::Result<Option<Mp4Box>> {
    let offset = reader.stream_position()?;

    // Read box size
    let size = match read_u32(reader) {
        Ok(sz) => sz as u64,
        Err(_) => return Ok(None), // Reached end of file
    };

    // Read box type (4-character code)
    let mut box_type_buf = [0; 4];
    reader.read_exact(&mut box_type_buf)?;
    let box_type = String::from_utf8_lossy(&box_type_buf).to_string();

    // Handle extended size
    let actual_size = if size == 1 {
        let mut ext_buf = [0; 8];
        reader.read_exact(&mut ext_buf)?;
        u64::from_be_bytes(ext_buf)
    } else {
        size
    };

    Ok(Some(Mp4Box {
        box_type,
        size: actual_size,
        offset,
    }))
}

/// Recursively parses MP4 boxes
pub fn parse_mp4_boxes(reader: &mut (impl Read + Seek), depth: usize, end: u64) -> io::Result<()> {
    while reader.stream_position()? < end {
        if let Some(mp4_box) = read_box(reader)? {
            // Print with indentation for hierarchy
            let indent = "↳ ".repeat(depth);
            println!("{indent}Box: {} (size: {}) at offset {}", mp4_box.box_type, mp4_box.size, mp4_box.offset);

            // Known container boxes that can nest other boxes
            let container_types = ["moov", "trak", "mdia", "minf", "stbl"];

            if container_types.contains(&mp4_box.box_type.as_str()) {
                let _current_pos = reader.stream_position()?;
                let next_box_end = mp4_box.offset + mp4_box.size;
                parse_mp4_boxes(reader, depth + 1, next_box_end)?;
                reader.seek(SeekFrom::Start(next_box_end))?;
            } else {
                reader.seek(SeekFrom::Start(mp4_box.offset + mp4_box.size))?;
            }
        } else {
            break;
        }
    }

    Ok(())
}
