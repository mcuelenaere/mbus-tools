const SINGLE_CHAR: u8 = 0xE5;
const SHORT_START: u8 = 0x10;
const LONG_START: u8 = 0x68;
const FRAME_END: u8 = 0x16;

#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    Single,
    Short {
        control: u8,
        address: u8,
    },
    Control {
        control: u8,
        address: u8,
        control_information: u8,
    },
    Long {
        control: u8,
        address: u8,
        control_information: u8,
        data: Vec<u8>,
    },
}

impl Frame {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, parser::ParseError> {
        let (_, frame) = parser::parse_frame(bytes)?;
        Ok(frame)
    }

    pub fn iter_bytes(&self) -> iterator::FrameIterator<'_> {
        iterator::FrameIterator::new(&self)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.iter_bytes().collect::<Vec<u8>>()
    }
}

pub type ParseError = parser::ParseError;

impl<'a> TryFrom<&'a [u8]> for Frame {
    type Error = ParseError;

    fn try_from(bytes: &'a [u8]) -> Result<Self, Self::Error> {
        Frame::from_bytes(bytes)
    }
}

mod iterator;
mod parser;
mod utils;
