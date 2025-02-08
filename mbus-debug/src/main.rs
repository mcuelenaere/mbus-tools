use clap::Parser;
use color_eyre::eyre::{Context, Result};
use futures_util::StreamExt;
use mbus_codec::MbusCodec;
use tokio_serial::SerialPortBuilderExt;
use tokio_util::codec::Decoder;
use tracing::{debug, info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[command()]
struct Args {
    #[arg(long, default_value = "info")]
    log_level: Level,

    #[arg(long, value_name = "TTY", value_hint = clap::ValueHint::FilePath)]
    tty_path: String,

    #[arg(short, long, default_value_t = 2400)]
    serial_baudrate: u32,
}

fn open_serial(path: String, baudrate: u32) -> Result<tokio_serial::SerialStream> {
    debug!("Opening serial port {} (baudrate={})", path, baudrate);

    let serial = tokio_serial::new(path, baudrate)
        .data_bits(tokio_serial::DataBits::Eight)
        .stop_bits(tokio_serial::StopBits::One)
        .parity(tokio_serial::Parity::Even)
        .flow_control(tokio_serial::FlowControl::None)
        .open_native_async()?;
    Ok(serial)
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    tracing::subscriber::set_global_default(
        FmtSubscriber::builder()
            .with_max_level(args.log_level)
            .finish(),
    )?;

    let serial = open_serial(args.tty_path, args.serial_baudrate)
        .with_context(|| "Failed to open external master port")?;

    let mut codec = MbusCodec::default().framed(serial);

    while let Some(frame) = codec.next().await {
        let frame = frame?;
        info!("Received frame: {:?}", frame);
    }

    Ok(())
}
