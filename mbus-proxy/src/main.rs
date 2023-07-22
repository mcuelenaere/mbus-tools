use anyhow::{Context, Result};
use clap::Parser;
use mbus_codec::MbusCodec;
use tokio::signal;
use tokio_serial::SerialPortBuilderExt;
use tokio_util::codec::Decoder;
use tokio_util::sync::CancellationToken;

mod mbus_codec;
mod multiplexer;

#[derive(Parser, Debug)]
#[command()]
struct Args {
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
    let serial = tokio_serial::new(path, baudrate)
        .data_bits(tokio_serial::DataBits::Eight)
        .stop_bits(tokio_serial::StopBits::One)
        .parity(tokio_serial::Parity::Even)
        .flow_control(tokio_serial::FlowControl::None)
        .open_native_async()?;
    Ok(serial)
}

fn spawn_sigint_watcher(token: CancellationToken) {
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("failed to listen for SIGINT");
        token.cancel();
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let external_master = open_serial(args.tty_path_external_master, args.serial_baudrate)
        .with_context(|| "Failed to open external master port")?;
    let heater = open_serial(args.tty_path_heater, args.serial_baudrate)
        .with_context(|| "Failed to open heater port")?;
    let wmbusmeters = open_serial(args.tty_path_wmbusmeters, args.serial_baudrate)
        .with_context(|| "Failed to open wmbusmeters port")?;

    let mut external_master = MbusCodec.framed(external_master);
    let mut heater = MbusCodec.framed(heater);
    let mut wmbusmeters = MbusCodec.framed(wmbusmeters);
    let token = CancellationToken::new();

    spawn_sigint_watcher(token.clone());

    while !token.is_cancelled() {
        multiplexer::multiplex_single_op(&mut external_master, &mut heater, &mut wmbusmeters)
            .await?;
    }

    Ok(())
}
