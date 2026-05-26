// Hand-rolled NIfTI-1 parser. Single-file (.nii) only, little-endian, int16.
//
// The NIfTI-1 header is exactly 348 bytes. We only need a handful of fields:
//   off  size  what
//    0   4     sizeof_hdr (must equal 348)
//   40   16    dim[8] as i16 — dim[0]=ndim, dim[1..=ndim] are sizes
//   70   2     datatype  (4 = DT_INT16)
//   72   2     bitpix    (16)
//   76   32    pixdim[8] as f32 — pixdim[1..=3] are voxel spacing in mm
//  108   4     vox_offset (f32, byte offset to voxel data — 352 for single-file)
//  344   4     magic "n+1\0"

pub struct Volume {
    pub dims: [u32; 3],
    pub spacing: [f32; 3],
    // Voxels rescaled to 0..=255, in x-fastest order.
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub enum NiftiError {
    TooShort,
    BadHeaderSize(i32),
    BadMagic([u8; 4]),
    UnsupportedDatatype(i16),
    BadDims,
    Truncated { need: usize, have: usize },
}

impl std::fmt::Display for NiftiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "file shorter than 348-byte header"),
            Self::BadHeaderSize(n) => write!(f, "sizeof_hdr = {}, expected 348", n),
            Self::BadMagic(m) => write!(f, "magic = {:?}, expected b\"n+1\\0\"", m),
            Self::UnsupportedDatatype(d) => write!(f, "datatype {} not supported (want 4 = int16)", d),
            Self::BadDims => write!(f, "dim[0] is not 3"),
            Self::Truncated { need, have } => write!(f, "voxel data truncated: need {}, have {}", need, have),
        }
    }
}

impl std::error::Error for NiftiError {}

fn rd_i16(b: &[u8], off: usize) -> i16 { i16::from_le_bytes([b[off], b[off + 1]]) }
fn rd_i32(b: &[u8], off: usize) -> i32 { i32::from_le_bytes(b[off..off + 4].try_into().unwrap()) }
fn rd_f32(b: &[u8], off: usize) -> f32 { f32::from_le_bytes(b[off..off + 4].try_into().unwrap()) }

pub fn parse(buf: &[u8]) -> Result<Volume, NiftiError> {
    if buf.len() < 348 {
        return Err(NiftiError::TooShort);
    }

    let sizeof_hdr = rd_i32(buf, 0);
    if sizeof_hdr != 348 {
        return Err(NiftiError::BadHeaderSize(sizeof_hdr));
    }

    let magic = [buf[344], buf[345], buf[346], buf[347]];
    if &magic != b"n+1\0" {
        return Err(NiftiError::BadMagic(magic));
    }

    let ndim = rd_i16(buf, 40);
    if ndim < 3 {
        return Err(NiftiError::BadDims);
    }
    let nx = rd_i16(buf, 42) as u32;
    let ny = rd_i16(buf, 44) as u32;
    let nz = rd_i16(buf, 46) as u32;

    let datatype = rd_i16(buf, 70);
    if datatype != 4 {
        return Err(NiftiError::UnsupportedDatatype(datatype));
    }

    let dx = rd_f32(buf, 80);
    let dy = rd_f32(buf, 84);
    let dz = rd_f32(buf, 88);

    let vox_offset = rd_f32(buf, 108) as usize;

    let n_voxels = (nx as usize) * (ny as usize) * (nz as usize);
    let need = vox_offset + n_voxels * 2;
    if buf.len() < need {
        return Err(NiftiError::Truncated { need, have: buf.len() });
    }

    // Percentile-based windowing. The handful of bright outliers in MRI (vessel
    // edges, noise spikes) wreck a naive min/max rescale — brain tissue then
    // ends up squashed into the bottom 30% of the output range and a transfer
    // function tuned for it instead catches scalp/marrow. We use a histogram
    // over the i16 range and pick [p1, p99.5] as the input window.
    let raw = &buf[vox_offset..vox_offset + n_voxels * 2];

    // Histogram across the i16 range, biased to nonneg (MRI magnitude is >= 0).
    const BINS: usize = 1 << 12; // 4096 bins
    let mut max_seen = 0i32;
    for chunk in raw.chunks_exact(2) {
        let v = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
        if v > max_seen { max_seen = v; }
    }
    let max_seen = max_seen.max(1) as f32;
    let mut hist = vec![0u32; BINS];
    for chunk in raw.chunks_exact(2) {
        let v = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
        if v <= 0 { continue; }
        let b = ((v as f32 / max_seen) * (BINS - 1) as f32) as usize;
        hist[b.min(BINS - 1)] += 1;
    }
    let total: u64 = hist.iter().map(|&c| c as u64).sum();
    let want_lo = (total as f64 * 0.01) as u64;
    let want_hi = (total as f64 * 0.995) as u64;
    let mut cum = 0u64;
    let mut bin_lo = 0usize;
    let mut bin_hi = BINS - 1;
    for (i, &c) in hist.iter().enumerate() {
        cum += c as u64;
        if cum >= want_lo && bin_lo == 0 { bin_lo = i; }
        if cum >= want_hi { bin_hi = i; break; }
    }
    let lo = (bin_lo as f32 / (BINS - 1) as f32) * max_seen;
    let hi = (bin_hi as f32 / (BINS - 1) as f32) * max_seen;
    let range = (hi - lo).max(1.0);

    let mut data = Vec::with_capacity(n_voxels);
    for chunk in raw.chunks_exact(2) {
        let v = i16::from_le_bytes([chunk[0], chunk[1]]) as f32;
        let f = ((v - lo) / range * 255.0).clamp(0.0, 255.0);
        data.push(f as u8);
    }

    Ok(Volume {
        dims: [nx, ny, nz],
        spacing: [dx, dy, dz],
        data,
    })
}
