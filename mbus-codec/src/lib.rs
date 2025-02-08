use bytes::{Buf, BufMut, BytesMut};
use mbus::{Frame, ParseError, ParseSizeNeeded};
use std::io::{Error, ErrorKind};
use tokio_util::codec::{Decoder, Encoder};
use tracing::trace;

#[derive(Default)]
pub struct MbusCodec {
    needed_bytes: usize,
}

impl Decoder for MbusCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < self.needed_bytes {
            return Ok(None);
        }

        match Frame::try_parse(src.chunk()) {
            Ok((bytes_read, frame)) => {
                trace!("Decoded frame {:?}", frame);

                src.advance(bytes_read);
                self.needed_bytes = 0;
                Ok(Some(frame))
            }
            Err(ParseError::Incomplete(ParseSizeNeeded::Size(min))) => {
                self.needed_bytes = min.into();
                Ok(None)
            }
            Err(ParseError::Incomplete(_)) => Ok(None),
            Err(err) => Err(Error::new(ErrorKind::InvalidData, err)),
        }
    }
}

impl Encoder<Frame> for MbusCodec {
    type Error = Error;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        trace!("Encoding frame {:?}", item);

        for byte in item.iter_bytes() {
            dst.put_u8(byte);
        }

        Ok(())
    }
}
