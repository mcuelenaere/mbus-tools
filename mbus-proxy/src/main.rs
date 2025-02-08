use clap::Parser;
use color_eyre::eyre::{Context, Result};
use futures_util::SinkExt;
use mbus::Frame;
use mbus_codec::MbusCodec;
use tokio::signal;
use tokio_serial::SerialPortBuilderExt;
use tokio_util::codec::Decoder;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, Level};
use tracing_subscriber::FmtSubscriber;

mod multiplexer;

#[derive(Parser, Debug)]
#[command()]
struct Args {
    #[arg(long, default_value = "info")]
    log_level: Level,

    #[arg(long, value_name = "TTY", value_hint = clap::ValueHint::FilePath)]
    tty_path_external_master: String,

    #[arg(long, value_name = "TTY", value_hint = clap::ValueHint::FilePath)]
    tty_path_heater: String,

    #[arg(long, value_name = "TTY", value_hint = clap::ValueHint::FilePath)]
    tty_path_wmbusmeters: String,

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

fn spawn_sigint_watcher(token: CancellationToken) {
    debug!("Spawning SIGINT watcher");
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("failed to listen for SIGINT");
        info!("SIGINT received, shutting down");
        token.cancel();
    });
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

    let external_master = open_serial(args.tty_path_external_master, args.serial_baudrate)
        .with_context(|| "Failed to open external master port")?;
    let heater = open_serial(args.tty_path_heater, args.serial_baudrate)
        .with_context(|| "Failed to open heater port")?;
    let wmbusmeters = open_serial(args.tty_path_wmbusmeters, args.serial_baudrate)
        .with_context(|| "Failed to open wmbusmeters port")?;

    let mut external_master = MbusCodec::default().framed(external_master);
    let mut heater = MbusCodec::default().framed(heater);
    let mut wmbusmeters = MbusCodec::default().framed(wmbusmeters);
    let token = CancellationToken::new();

    spawn_sigint_watcher(token.clone());

    info!("Initializing all slaves");
    heater
        .send(Frame::Short {
            control: 0x40,
            address: 0x0,
        })
        .await?;

    info!("Starting main loop");
    while !token.is_cancelled() {
        multiplexer::multiplex_single_op(
            token.clone(),
            &mut external_master,
            &mut heater,
            &mut wmbusmeters,
        )
        .await?;
    }

    Ok(())
}
