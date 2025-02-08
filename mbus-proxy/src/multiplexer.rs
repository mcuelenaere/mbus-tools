use color_eyre::eyre::{Context, Result};
use std::time::Duration;

use futures_util::stream::StreamExt;
use futures_util::{FutureExt, Sink, SinkExt, Stream};
use mbus::Frame;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

const SND_NKE: u8 = 0x40;
const SND_UD: u8 = 0x73;
const REQ_UD2: u8 = 0x7B;

async fn forward_frame<S>(frame: Frame, origin: &mut S, destination: &mut S) -> Result<()>
where
    S: Stream<Item = std::result::Result<Frame, std::io::Error>>
        + Sink<Frame, Error = std::io::Error>
        + Unpin,
{
    // forward to heater
    debug!("Forwarding frame {:?} to destination", frame);
    destination.send(frame).await?;

    // read response or timeout after 50ms
    let resp = tokio::time::timeout(
        Duration::from_secs(2),
        destination.next().map(|r| r.unwrap()),
    )
    .await??;

    debug!(
        "Received response {:?} from destination, forwarding it to the origin",
        resp
    );

    // reply
    origin.send(resp).await?;

    Ok(())
}

pub async fn multiplex_single_op<S>(
    token: CancellationToken,
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
            debug!("Received frame {:?} from external master", frame);

            match frame {
                Frame::Short { control, address } if control == SND_NKE && (address == 0xFF || address == 0x5A) => {
                    external_master.send(Frame::Single).await?;
                }
                Frame::Short { address, .. } | Frame::Long { address, .. } | Frame::Control { address, .. } if address == 0x5A => {
                    forward_frame(frame, external_master, heater).await?;
                }
                Frame::Short { .. } | Frame::Long { .. } | Frame::Control { .. } => {
                    // ignore, this is not for us
                    info!("Received frame from external master for a slave that we are not familiar with: {:?}", frame)
                },
                _ => {
                    error!("Received unexpected frame from external master: {:?}", frame);
                }
            }
        }
        Some(result) = wmbusmeters.next() => {
            let frame = result.with_context(|| "Failed reading frame from wmbusmeters")?;
            debug!("Received frame {:?} from wmbusmeters", frame);

            match frame {
                Frame::Short { control, address } if control == SND_NKE && (address == 0x0 || address == 0xFD) => {
                    wmbusmeters.send(Frame::Single).await?;
                }
                Frame::Long { control, address, data, .. } if control == SND_UD && address == 0xFD && data == b"\x87\x93\x27\x68\xff\xff\xff\xff" => {
                    wmbusmeters.send(Frame::Single).await?;
                }
                Frame::Short { control, address } if address == 0xFD => {
                    forward_frame(Frame::Short {
                        control,
                        address: 0x5A,
                    }, wmbusmeters, heater).await?;
                },
                _ => {
                    error!("Received unexpected frame from wmbusmeters: {:?}", frame);
                }
            }
        }
        Some(result) = heater.next() => {
            let frame = result.with_context(|| "Failed reading frame from heater")?;

            error!("Received unexpected frame from heater: {:?}", frame);
        }
        _ = token.cancelled() => {
            debug!("Cancellation token received, shutting down");
            return Ok(());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbus_codec::MbusCodec;
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
            MbusCodec::default().framed(self.0.build())
        }
    }

    #[tokio::test]
    async fn test_master_send_nke() -> Result<()> {
        let mut external_master = MockBuilder::new()
            .read(Frame::Short {
                control: SND_NKE,
                address: 0x0,
            })
            .write(Frame::Single)
            .build();
        let mut heater = MockBuilder::new().build();
        let mut wmbusmeter = MockBuilder::new().build();

        multiplex_single_op(
            CancellationToken::new(),
            &mut external_master,
            &mut heater,
            &mut wmbusmeter,
        )
        .await?;
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
                address: 0x0,
            })
            .write(Frame::Single)
            .build();

        multiplex_single_op(
            CancellationToken::new(),
            &mut external_master,
            &mut heater,
            &mut wmbusmeter,
        )
        .await?;
        assert!(wmbusmeter.next().await.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_master_forward_req_ud2() -> Result<()> {
        let mut external_master = MockBuilder::new()
            .read(Frame::Short {
                control: REQ_UD2,
                address: 0x5A,
            })
            .write(Frame::Long {
                control: 0x00,
                address: 0x5A,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x00,
            })
            .build();
        let mut heater = MockBuilder::new()
            .write(Frame::Short {
                control: REQ_UD2,
                address: 0x5A,
            })
            .read(Frame::Long {
                control: 0x00,
                address: 0x5A,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x00,
            })
            .build();
        let mut wmbusmeter = MockBuilder::new().build();

        multiplex_single_op(
            CancellationToken::new(),
            &mut external_master,
            &mut heater,
            &mut wmbusmeter,
        )
        .await?;
        assert!(heater.next().await.is_none());
        assert!(external_master.next().await.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_mux_two_req_ud2s() -> Result<()> {
        let mut external_master = MockBuilder::new()
            .read(Frame::Short {
                control: REQ_UD2,
                address: 0x5A,
            })
            .write(Frame::Long {
                control: 0x00,
                address: 0x5A,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x00,
            })
            .build();
        let mut heater = MockBuilder::new()
            .write(Frame::Short {
                control: REQ_UD2,
                address: 0x5A,
            })
            .read(Frame::Long {
                control: 0x00,
                address: 0x5A,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x00,
            })
            .write(Frame::Short {
                control: REQ_UD2,
                address: 0x5A,
            })
            .read(Frame::Long {
                control: 0x00,
                address: 0x5A,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x01,
            })
            .build();
        let mut wmbusmeter = MockBuilder::new()
            .read(Frame::Short {
                control: REQ_UD2,
                address: 0xFD,
            })
            .write(Frame::Long {
                control: 0x00,
                address: 0x5A,
                data: vec![0xCA, 0xFE, 0xBA, 0xBE],
                control_information: 0x01,
            })
            .build();

        multiplex_single_op(
            CancellationToken::new(),
            &mut external_master,
            &mut heater,
            &mut wmbusmeter,
        )
        .await?;
        multiplex_single_op(
            CancellationToken::new(),
            &mut external_master,
            &mut heater,
            &mut wmbusmeter,
        )
        .await?;
        assert!(heater.next().await.is_none());
        assert!(external_master.next().await.is_none());
        assert!(wmbusmeter.next().await.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_cancel() -> Result<()> {
        let mut external_master = MockBuilder::new().build();
        let mut heater = MockBuilder::new().build();
        let mut wmbusmeter = MockBuilder::new().build();
        let token = CancellationToken::new();
        token.cancel();

        multiplex_single_op(
            token.clone(),
            &mut external_master,
            &mut heater,
            &mut wmbusmeter,
        )
        .await?;

        Ok(())
    }
}
