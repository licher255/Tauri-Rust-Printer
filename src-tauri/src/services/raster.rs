//! PWG Raster and URF decoding into top-down 24-bit BMP pages.
//! Wire layouts: PWG 5102.4; URF interoperability checked against
//! https://github.com/OpenPrinting/libcups/blob/master/cups/raster-stream.c
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn byte(input: &mut &[u8]) -> io::Result<u8> {
    let mut value = [0];
    input.read_exact(&mut value)?;
    Ok(value[0])
}
fn be32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// Decodes only advertised sGray8 and sRGB24. Limits apply to decompressed
/// bytes across ALL pages, not just the small compressed upload.
pub fn decode(data: &[u8], directory: &Path) -> io::Result<Vec<PathBuf>> {
    let urf = data.starts_with(b"UNIRAST\0");
    let mut input = if urf {
        data.get(12..)
            .ok_or_else(|| invalid("Truncated URF header"))?
    } else if data.starts_with(b"RaS2") {
        &data[4..]
    } else {
        return Err(invalid("Unknown raster signature"));
    };
    let expected_pages = if urf { be32(data, 8) } else { 0 };
    let mut paths = Vec::new();
    let mut total_bytes = 0u64;
    while !input.is_empty() {
        if paths.len() >= 500 {
            return Err(invalid("Too many raster pages"));
        }
        let size = if urf { 32 } else { 1796 };
        let header = input
            .get(..size)
            .ok_or_else(|| invalid("Truncated page header"))?;
        let (width, height, dpi, components) = if urf {
            let components = match (header[0], header[1]) {
                (8, 0) => 1,
                (24, 1) => 3,
                _ => return Err(invalid("Unsupported URF color space or bit depth")),
            };
            (
                be32(header, 12),
                be32(header, 16),
                be32(header, 20),
                components,
            )
        } else {
            let components = match (be32(header, 384), be32(header, 388), be32(header, 400)) {
                (8, 8, 18) => 1,
                (8, 24, 19) => 3,
                _ => return Err(invalid("Unsupported PWG color space or bit depth")),
            };
            if be32(header, 396) != 0 {
                return Err(invalid("Only chunky raster is supported"));
            }
            let width = be32(header, 372);
            if be32(header, 392) as u64 != width as u64 * components {
                return Err(invalid("Invalid PWG row length"));
            }
            (width, be32(header, 376), be32(header, 276), components)
        };
        if width == 0 || height == 0 || width > 20000 || height > 20000 || dpi == 0 || dpi > 2400 {
            return Err(invalid("Invalid raster dimensions or resolution"));
        }
        input = &input[size..];
        let stride = (width as u64 * 3 + 3) & !3;
        let image_bytes = stride * height as u64;
        total_bytes += image_bytes;
        if total_bytes > 512 * 1024 * 1024 {
            return Err(invalid("Decoded raster exceeds 512 MiB"));
        }
        let path = directory.join(format!("page-{}.bmp", paths.len()));
        let mut file = std::fs::File::create(&path)?;
        let mut bmp = vec![0u8; 54];
        bmp[..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&((image_bytes + 54) as u32).to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&width.to_le_bytes());
        bmp[22..26].copy_from_slice(&(-(height as i32)).to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        let ppm = (dpi as f64 / 0.0254).round() as u32;
        bmp[38..42].copy_from_slice(&ppm.to_le_bytes());
        bmp[42..46].copy_from_slice(&ppm.to_le_bytes());
        file.write_all(&bmp)?;
        let row_size = width as usize * components as usize;
        let mut y = 0;
        while y < height {
            let repeats = byte(&mut input)? as u32 + 1;
            if y + repeats > height {
                return Err(invalid("Row repeat exceeds page"));
            }
            let mut row = Vec::with_capacity(row_size);
            while row.len() < row_size {
                let control = byte(&mut input)?;
                if control == 128 {
                    row.resize(row_size, 255);
                    break;
                }
                let pixels = if control < 128 {
                    control as usize + 1
                } else {
                    257 - control as usize
                };
                let count = pixels * components as usize;
                if count > row_size - row.len() {
                    return Err(invalid("Run exceeds row"));
                }
                if control < 128 {
                    let mut pixel = vec![0; components as usize];
                    input.read_exact(&mut pixel)?;
                    for _ in 0..pixels {
                        row.extend_from_slice(&pixel);
                    }
                } else {
                    let start = row.len();
                    row.resize(start + count, 0);
                    input.read_exact(&mut row[start..])?;
                }
            }
            let mut output = Vec::with_capacity(stride as usize);
            for pixel in row.chunks_exact(components as usize) {
                if components == 1 {
                    output.extend_from_slice(&[pixel[0]; 3]);
                } else {
                    output.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                }
            }
            output.resize(stride as usize, 0);
            for _ in 0..repeats {
                file.write_all(&output)?;
            }
            y += repeats;
        }
        paths.push(path);
    }
    if paths.is_empty()
        || (urf
            && expected_pages != 0
            && expected_pages != u32::MAX
            && expected_pages as usize != paths.len())
    {
        return Err(invalid("Raster page count mismatch"));
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn urf() -> Vec<u8> {
        let mut data = b"UNIRAST\0".to_vec();
        data.extend(1u32.to_be_bytes());
        let mut header = [0; 32];
        header[0] = 24;
        header[1] = 1;
        header[12..16].copy_from_slice(&2u32.to_be_bytes());
        header[16..20].copy_from_slice(&2u32.to_be_bytes());
        header[20..24].copy_from_slice(&300u32.to_be_bytes());
        data.extend(header);
        data.extend([1, 255, 255, 0, 0, 0, 255, 0]);
        data
    }
    #[test]
    fn decompresses_rgb_literals_and_repeated_rows() {
        let temp = tempfile::tempdir().unwrap();
        let paths = decode(&urf(), temp.path()).unwrap();
        let bmp = std::fs::read(&paths[0]).unwrap();
        assert_eq!(
            &bmp[54..],
            &[0, 0, 255, 0, 255, 0, 0, 0, 0, 0, 255, 0, 255, 0, 0, 0]
        );
        for len in 0..urf().len() {
            assert!(decode(&urf()[..len], temp.path()).is_err());
        }
        let mut bad = urf();
        bad[44] = 255;
        assert!(decode(&bad, temp.path()).is_err());
    }
    #[test]
    fn pwg_grayscale_and_limits() {
        let temp = tempfile::tempdir().unwrap();
        let mut data = b"RaS2".to_vec();
        let mut header = vec![0; 1796];
        for (offset, value) in [
            (276, 300u32),
            (280, 300),
            (372, 1),
            (376, 1),
            (384, 8),
            (388, 8),
            (392, 1),
            (400, 18),
        ] {
            header[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        data.extend(&header);
        data.extend([0, 0, 64]);
        let paths = decode(&data, temp.path()).unwrap();
        assert_eq!(&std::fs::read(&paths[0]).unwrap()[54..], &[64, 64, 64, 0]);
        data[4 + 372..4 + 376].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode(&data, temp.path()).is_err());
    }
}
