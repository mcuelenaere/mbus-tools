use color_eyre::eyre::{bail, Context, Result};

use futures_util::stream::StreamExt;
use futures_util::{Sink, SinkExt, Stream};
use mbus::Frame;

const SND_NKE: u8 = 0x40;
const REQ_UD2: u8 = 0x7B;

async fn forward_frame<S>(frame: Frame, origin: &mut S, destination: &mut S) -> Result<()>
where
    S: Stream<Item = std::result::Result<Frame, std::io::Error>>
        + Sink<Frame, Error = std::io::Error>
        + Unpin,
{
    // forward to heater
    destination.send(frame).await?;

    // wait for response
    // TODO: add timeout mechanism
    let resp = destination.next().await.unwrap()?;

    // reply
    origin.send(resp).await?;

    Ok(())
}

pub async fn multiplex_single_op<S>(
    external_master: &mut S,
    heater: &mut S,
    wmbusmeters: &mut S,
) -> Result<()>
where
    S: Stream<Item = std::result::Result<Frame, std::io::Error>>
        + Sink<Frame, Error = std::io::Error>
        + Unpin,
{
    tokio::select! {
        biased;

        Some(result) = external_master.next() => {
            let frame = result.with_context(|| "Failed reading frame from external master")?;

            match frame {
                Frame::Short { control: SND_NKE, .. } => {
                    external_master.send(Frame::Single).await?;
                },
                Frame::Short { control: REQ_UD2, address } => {
                    if address == 0x47 {
                        forward_frame(frame, external_master, heater).await?;
                    } else {
                        // ignore, this is not for us
                    }
                },
                _ => {
                    bail!("Received unexpected frame from external master: {:?}", frame);
                }
            }
        }
        Some(result) = wmbusmeters.next() => {
            let frame = result.with_context(|| "Failed reading frame from wmbusmeters")?;

            match frame {
                Frame::Short { control: SND_NKE, .. } => {
                    wmbusmeters.send(Frame::Single).await?;
                },
                _ => {
                    forward_frame(frame, wmbusmeters, heater).await?;
                }
            }
        }
        Some(result) = heater.next() => {
            let frame = result.with_context(|| "Failed reading frame from heater")?;

            bail!("Received unexpected frame from heater: {:?}", frame);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mbus_codec::MbusCodec;
    use tokio_util::codec::{Decoder, Framed};

    #[derive(Clone, Debug)]
    struct MockBuilder(tokio_test::io::Builder);

    impl MockBuilder {
        pub fn new() -> Self {
            MockBuilder(tokio_test::io::Builder::new())
        }

        pub fn read(&mut self, frame: Frame) -> &mut Self {
            self.0.read(frame.to_bytes().as_ref());
            self
        }

        pub fn write(&mut self, frame: Frame) -> &mut Self {
            self.0.write(frame.to_bytes().as_ref());
            self
        }

        pub fn build(&mut self) -> Framed<tokio_test::io::Mock, MbusCodec> {
            MbusCodec.framed(self.0.build())
        }
    }

    #[tokio::test]
    async fn test_master_send_nke() -> Result<()> {
        let mut external_master = MockBuilder::new()
            .read(Frame::Short {
                control: SND_NKE,
                address: 0xff,
            })
            .write(Frame::Single)
            .build();
        let mut heater = MockBuilder::new().build();
        let mut wmbusmeter = MockBuilder::new().build();

        multiplex_single_op(&mut external_master, &mut heater, &mut wmbusmeter).await?;
        assert!(external_master.next().await.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_wmbusmeter_send_nke() -> Result<()> {
        let mut external_master = MockBuilder::new().build();
        let mut heater = MockBuilder::new().build();
        let mut wmbusmeter = MockBuilder::new()
            .read(Frame::Short {
                control: SND_NKE,
                address: 0xff,
            })
            .write(Frame::Single)
            .build();

        multiplex_single_op(&mut external_master, &mut heater, &mut wmbusmeter).await?;
        assert!(wmbusmeter.next().await.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_master_forward_req_ud2() -> Result<()> {
        let mut external_master = MockBuilder::new()
            .read(Frame::Short {
                control: REQ_UD2,
                address: 0x47,
            })
            .write(Frame::Long {
                control: 0x00,
                address: 0x47,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x00,
            })
            .build();
        let mut heater = MockBuilder::new()
            .write(Frame::Short {
                control: REQ_UD2,
                address: 0x47,
            })
            .read(Frame::Long {
                control: 0x00,
                address: 0x47,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x00,
            })
            .build();
        let mut wmbusmeter = MockBuilder::new().build();

        multiplex_single_op(&mut external_master, &mut heater, &mut wmbusmeter).await?;
        assert!(heater.next().await.is_none());
        assert!(external_master.next().await.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_mux_two_req_ud2s() -> Result<()> {
        let mut external_master = MockBuilder::new()
            .read(Frame::Short {
                control: REQ_UD2,
                address: 0x47,
            })
            .write(Frame::Long {
                control: 0x00,
                address: 0x47,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x00,
            })
            .build();
        let mut heater = MockBuilder::new()
            .write(Frame::Short {
                control: REQ_UD2,
                address: 0x47,
            })
            .read(Frame::Long {
                control: 0x00,
                address: 0x47,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x00,
            })
            .write(Frame::Short {
                control: REQ_UD2,
                address: 0x47,
            })
            .read(Frame::Long {
                control: 0x00,
                address: 0x47,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x01,
            })
            .build();
        let mut wmbusmeter = MockBuilder::new()
            .read(Frame::Short {
                control: REQ_UD2,
                address: 0x47,
            })
            .write(Frame::Long {
                control: 0x00,
                address: 0x47,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x01,
            })
            .build();

        multiplex_single_op(&mut external_master, &mut heater, &mut wmbusmeter).await?;
        multiplex_single_op(&mut external_master, &mut heater, &mut wmbusmeter).await?;
        assert!(heater.next().await.is_none());
        assert!(external_master.next().await.is_none());
        assert!(wmbusmeter.next().await.is_none());

        Ok(())
    }
}
