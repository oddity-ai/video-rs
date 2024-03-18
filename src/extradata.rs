use crate::error::Error;

type Result<T> = std::result::Result<T, Error>;

/// Represents a borrowed byte stream representation of an H264 stream Sequence Parameter Set (SPS)
/// as defined in Section 7.3.2.1 in the Recommendation H.264.
///
/// For purposes of this crate, we don't deserialize the PPS into its constituent contents, and
/// provide to the caller only the bytes.
pub type Sps<'buf> = &'buf [u8];

/// Represents borrowed byte stream representations of the H264 stream Picture Parameter Sets (PPSs)
/// as defined in Section 7.3.2.2 in the Recommendation H.264.
///
/// Note that H.264 streams have one Sequence Parameter Set but can have one or more Picture
/// Parameter Sets.
///
/// For purposes of this crate, we don't deserialize the PPS into its constituent contents, and
/// provide to the caller only the PPS bytes.
pub type Pps<'buf> = Vec<&'buf [u8]>;

/// Extract the Sequence Parameter Set (SPS) and Picture Parameter Sets (PPSs) from an H.264 stream
/// `extradata` bytes (as provided by the `libavcodec` backend).
///
/// # Arguments
///
/// * `extradata_bytes` - Borrowed slice pointing to extradata bytes.
///
/// # Return value
///
/// `Sps` and `Pps` or error.
pub fn extract_parameter_sets_h264(extradata_bytes: &[u8]) -> Result<(Sps<'_>, Pps<'_>)> {
    if !extradata_bytes.is_empty() {
        match extradata_bytes[0] {
            0x00 => extract_parameter_sets_from_extradata_h264_avc_annexb(extradata_bytes),
            0x01 => extract_parameter_sets_from_extradata_h264_avcc(extradata_bytes),
            _ => Err(Error::InvalidExtraData),
        }
    } else {
        Err(Error::InvalidExtraData)
    }
}

/// Extract parameter sets from H264 stream in AVCC format. The AVCC format is most commonly used in
/// combination with the MP4 container format or any other format where it makes sense to include
/// the parameter sets in the beginning of the stream (non-live formats).
fn extract_parameter_sets_from_extradata_h264_avcc(bytes: &[u8]) -> Result<(Sps<'_>, Pps<'_>)> {
    if bytes.len() > 8 {
        let sps_size = u16::from_be_bytes([bytes[6], bytes[7]]);
        let sps = &bytes[8_usize..(8 + sps_size) as usize];

        let mut ppss = Vec::new();
        let pps_array = &bytes[(8 + sps_size) as usize..];
        if pps_array.len() > 1 {
            let pps_num = pps_array[0];
            let pps_array = &pps_array[1..];
            let mut pps_p = 0;
            for _ in 0..pps_num {
                if pps_array[pps_p..].len() < 2 {
                    return Err(Error::InvalidExtraData);
                }

                let pps_size = u16::from_be_bytes([pps_array[pps_p], pps_array[pps_p + 1]]);
                if pps_array[pps_p + 2..].len() < pps_size as usize {
                    return Err(Error::InvalidExtraData);
                }

                let pps = &pps_array[pps_p + 2..pps_p + 2 + pps_size as usize];
                ppss.push(pps);
                pps_p += 2 + pps_size as usize;
            }

            Ok((sps, ppss))
        } else {
            Err(Error::InvalidExtraData)
        }
    } else {
        Err(Error::InvalidExtraData)
    }
}

/// Extract parameter sets from H264 stream in Annex B format. The Annex B format is commonly used
/// in live-streaming contexts. For example, in combination with the MPEG-TS.
fn extract_parameter_sets_from_extradata_h264_avc_annexb(
    bytes: &[u8],
) -> Result<(Sps<'_>, Pps<'_>)> {
    let mut index_current = find_avc_start_code(bytes, 0).map(|(_, index_next)| index_next);

    let mut sps: Option<Sps<'_>> = None;
    let mut pps: Pps<'_> = Vec::new();

    while let Some(index) = index_current {
        let (end, index_next) = match find_avc_start_code(bytes, index) {
            Some((end, index_next)) => (end, Some(index_next)),
            None => (bytes.len(), None),
        };
        let nal = &bytes[index..end];
        let nal_type = nal[0] & 0x1f;
        match nal_type {
      0x07 /* SPS */ => sps = Some(nal),
      0x08 /* PPS */ => pps.push(nal),
      _ => {}
    };

        index_current = index_next;
    }

    if let Some(sps) = sps {
        Ok((sps, pps))
    } else {
        Err(Error::InvalidExtraData)
    }
}

/// The H.264 AVC spec defines a NAL start code to be either two zero bytes followed by a 0x01-byte
/// (allowed in Annex B format) or three zeros bytes followed by a 0x01-bytes (allowed in AVCC and
/// Annex B formats). This function will find the AVC start code (both formats) and return its
/// position.
///
/// # Arguments
///
/// * `bytes` - Byte slice to find start code in.
/// * `offset` - Offset in slice to start looking for start code.
///
/// # Return value
///
/// If a start code was found in the byte slice (starting from `offset`), then a tuple of `start`
/// and `end` is returned, where `start` is the index of the first byte of the start code, and `end`
/// is the index of the first byte after the start code. If no start code was found, it returns
/// `None`.
fn find_avc_start_code(bytes: &[u8], offset: usize) -> Option<(usize, usize)> {
    let part = &bytes[offset..];
    if part.len() >= 3 {
        for i in 0..(part.len() - 3) {
            if part[i..i + 3] == [0x00, 0x00, 0x01] {
                return Some((offset + i, offset + i + 3));
            } else if i + 4 <= part.len() && part[i..i + 4] == [0x00, 0x00, 0x00, 0x01] {
                return Some((offset + i, offset + i + 4));
            }
        }
        None
    } else {
        None
    }
}
