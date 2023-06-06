use crate::utils::calculate_checksum;
use crate::{Frame, FRAME_END, LONG_START, SHORT_START, SINGLE_CHAR};
use nom::{
    branch::alt,
    bytes::streaming::{tag, take},
    combinator::{cut, map_res},
    sequence::{tuple, Tuple},
    Err, IResult, Parser,
};

#[derive(Debug, Eq, PartialEq)]
pub enum FrameParseError {
    MalformedChecksum,
    InconsistentLengthValues,
    Nom(nom::error::ErrorKind),
}

impl<'a> nom::error::ParseError<&'a [u8]> for FrameParseError {
    fn from_error_kind(_: &'a [u8], kind: nom::error::ErrorKind) -> Self {
        Self::Nom(kind)
    }

    fn append(_: &'a [u8], _: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

impl<'a> nom::error::FromExternalError<&'a [u8], FrameParseError> for FrameParseError {
    fn from_external_error(_: &'a [u8], _: nom::error::ErrorKind, e: FrameParseError) -> Self {
        e
    }
}

fn tag_short_start(i: &[u8]) -> IResult<&[u8], &[u8], FrameParseError> {
    tag(&[SHORT_START])(i)
}

fn tag_long_start(i: &[u8]) -> IResult<&[u8], &[u8], FrameParseError> {
    tag(&[LONG_START])(i)
}

fn tag_frame_end(i: &[u8]) -> IResult<&[u8], &[u8], FrameParseError> {
    tag(&[FRAME_END])(i)
}

fn checksummed_buf<'a>(
    n: usize,
) -> impl FnMut(&'a [u8]) -> IResult<&'a [u8], &'a [u8], FrameParseError> {
    cut(map_res(take(n), |i: &[u8]| {
        let l = i.len();
        if calculate_checksum(&i[0..l - 1]) != i[l - 1] {
            Err(FrameParseError::MalformedChecksum)
        } else {
            Ok(&i[0..l - 1])
        }
    }))
}

fn length_value(i: &[u8]) -> IResult<&[u8], usize, FrameParseError> {
    cut(map_res(take(2usize), |i: &[u8]| {
        if i[0] != i[1] {
            Err(FrameParseError::InconsistentLengthValues)
        } else {
            Ok(i[0] as usize)
        }
    }))(i)
}

fn single(i: &[u8]) -> IResult<&[u8], Frame, FrameParseError> {
    tag(&[SINGLE_CHAR]).map(|_| Frame::Single).parse(i)
}

fn short_frame(i: &[u8]) -> IResult<&[u8], Frame, FrameParseError> {
    tuple((tag_short_start, checksummed_buf(3), tag_frame_end))
        .map(|(_, i, _)| Frame::Short {
            control: i[0],
            address: i[1],
        })
        .parse(i)
}

fn long_frame(i: &[u8]) -> IResult<&[u8], Frame, FrameParseError> {
    let (i, (_, length)) = (tag_long_start, length_value).parse(i)?;
    let (i, (_, buf, _)) = (tag_long_start, checksummed_buf(length + 1), tag_frame_end).parse(i)?;

    if length < 3 {
        return Err(Err::Failure(FrameParseError::InconsistentLengthValues));
    }

    let frame = if length == 3 {
        Frame::Control {
            control: buf[0],
            address: buf[1],
            control_information: buf[2],
        }
    } else {
        Frame::Long {
            control: buf[0],
            address: buf[1],
            control_information: buf[2],
            data: Vec::from(&buf[3..]),
        }
    };

    Ok((i, frame))
}

pub type ParseError = Err<FrameParseError>;
pub fn parse_frame(i: &[u8]) -> IResult<&[u8], Frame, FrameParseError> {
    alt((single, short_frame, long_frame))(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frame() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(Frame::from_bytes(b"\xe5")?, Frame::Single,);
        assert_eq!(
            Frame::from_bytes(b"\x10\x7b\x49\xc4\x16")?,
            Frame::Short {
                address: 0x49,
                control: 0x7B
            }
        );
        assert_eq!(
            Frame::from_bytes(b"\x68\x03\x03\x68\x53\xFE\xBD\x0E\x16")?,
            Frame::Control {
                address: 0xFE,
                control: 0x53,
                control_information: 0xBD,
            }
        );
        assert_eq!(
            Frame::from_bytes(b"\x68\x06\x06\x68\x53\xFE\x51\x01\x7A\x08\x25\x16")?,
            Frame::Long {
                address: 0xFE,
                control: 0x53,
                control_information: 0x51,
                data: (*b"\x01\x7A\x08").into()
            }
        );

        // faulty frames
        assert!(matches!(
            Frame::from_bytes(b"\x10\x7b\x49\xc5\x16"),
            Err(Err::Failure(FrameParseError::MalformedChecksum))
        ));
        assert!(matches!(
            Frame::from_bytes(b"\x68\x03\x02\x68\x53\xFE\xBD\x0E\x16"),
            Err(Err::Failure(FrameParseError::InconsistentLengthValues))
        ));

        Ok(())
    }
}
