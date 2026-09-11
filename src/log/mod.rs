use nu_ansi_term::{Color, Style};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
};

struct CustomLogFormat;

impl<S, N> FormatEvent<S, N> for CustomLogFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let meta = event.metadata();

        let now = chrono::Local::now().format("%d-%m-%Y %H:%M:%S");
        let grey = Style::new().dimmed();
        write!(writer, "{} | ", grey.paint(now.to_string()))?;

        let level = meta.level();
        let level_color = match *level {
            Level::ERROR => Color::Red.bold(),
            Level::WARN => Color::Yellow.bold(),
            Level::INFO => Color::Green.bold(),
            Level::DEBUG => Color::Blue.bold(),
            Level::TRACE => Color::Purple.bold(),
        };
        write!(writer, "{} | ", level_color.paint(level.as_str()))?;

        let cyan = Color::Cyan.normal();

        write!(writer, "{}", cyan.prefix())?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        write!(writer, "{}", cyan.suffix())?;

        writeln!(writer)
    }
}

pub fn init() -> tracing_appender::non_blocking::WorkerGuard {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into());

    let file_appender = tracing_appender::rolling::never(".", "kroma.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().event_format(CustomLogFormat))
        .with(file_layer)
        .init();

    guard
}
