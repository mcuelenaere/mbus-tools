use crate::utils::calculate_checksum;
use crate::*;

pub struct FrameIterator<'a> {
    frame: &'a Frame,
    index: usize,
}

impl<'a> FrameIterator<'a> {
    pub(crate) fn new(frame: &'a Frame) -> Self {
        Self { frame, index: 0 }
    }
}

impl Iterator for FrameIterator<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        match self.frame {
            Frame::Single => {
                if self.index == 0 {
                    self.index += 1;
                    Some(SINGLE_CHAR)
                } else {
                    None
                }
            }
            Frame::Short { address, control } => {
                let b = match self.index {
                    0 => SHORT_START,
                    1 => *control,
                    2 => *address,
                    3 => calculate_checksum(&[*control, *address]),
                    4 => FRAME_END,
                    _ => return None,
                };
                self.index += 1;
                Some(b)
            }
            Frame::Control {
                address,
                control,
                control_information,
            } => {
                let b = match self.index {
                    0 => LONG_START,
                    1 | 2 => 3,
                    3 => LONG_START,
                    4 => *control,
                    5 => *address,
                    6 => *control_information,
                    7 => calculate_checksum(&[*control, *address, *control_information]),
                    8 => FRAME_END,
                    _ => return None,
                };
                self.index += 1;
                Some(b)
            }
            Frame::Long {
                address,
                control,
                control_information,
                data,
            } => {
                let b = match self.index {
                    0 => LONG_START,
                    1 | 2 => (data.len() + 3) as u8,
                    3 => LONG_START,
                    4 => *control,
                    5 => *address,
                    6 => *control_information,
                    _ => {
                        if self.index <= 6 + data.len() {
                            data[self.index - 7]
                        } else if self.index == 6 + data.len() + 1 {
                            calculate_checksum(
                                ([*control, *address, *control_information])
                                    .iter()
                                    .chain(data.iter()),
                            )
                        } else if self.index == 6 + data.len() + 2 {
                            FRAME_END
                        } else {
                            return None;
                        }
                    }
                };
                self.index += 1;
                Some(b)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iterator() {
        assert_eq!(Frame::Single.to_bytes(), b"\xe5",);
        assert_eq!(
            Frame::Short {
                address: 0x49,
                control: 0x7B
            }
            .to_bytes(),
            b"\x10\x7b\x49\xc4\x16",
        );
        assert_eq!(
            Frame::Control {
                address: 0xFE,
                control: 0x53,
                control_information: 0xBD,
            }
            .to_bytes(),
            b"\x68\x03\x03\x68\x53\xFE\xBD\x0E\x16",
        );
        assert_eq!(
            Frame::Long {
                address: 0xFE,
                control: 0x53,
                control_information: 0x51,
                data: (*b"\x01\x7A\x08").into()
            }
            .to_bytes(),
            b"\x68\x06\x06\x68\x53\xFE\x51\x01\x7A\x08\x25\x16",
        );
    }
}
