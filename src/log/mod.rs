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

        // --- 1. Timestamp: DD-MM-YYYY HH:mm:ss (Grey) ---
        let now = chrono::Local::now().format("%d-%m-%Y %H:%M:%S");
        let grey = Style::new().dimmed();
        write!(writer, "{} | ", grey.paint(now.to_string()))?;

        // --- 2. LOG_KIND: (Color based on level) ---
        let level = meta.level();
        let level_color = match *level {
            Level::ERROR => Color::Red.bold(),
            Level::WARN => Color::Yellow.bold(),
            Level::INFO => Color::Green.bold(),
            Level::DEBUG => Color::Blue.bold(),
            Level::TRACE => Color::Purple.bold(),
        };
        write!(writer, "{} | ", level_color.paint(level.as_str()))?;

        // --- 3. Variables / Span Context (Blue) ---
        let blue = Style::new().fg(Color::Cyan);
        let mut has_context = false;

        // Extract fields attached to parent spans (e.g. method=GET, uri=/test from tower_http)
        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                let ext = span.extensions();
                if let Some(fields) = ext.get::<tracing_subscriber::fmt::FormattedFields<N>>()
                    && !fields.is_empty()
                {
                    write!(writer, "{}", blue.paint(fields.as_str()))?;
                    has_context = true;
                }
            }
        }
        if !has_context {
            write!(writer, "{}", blue.paint("none"))?;
        }
        write!(writer, " | ")?;

        // --- 4. Message (Cyan) ---
        let cyan = Color::Cyan.normal();

        // Format the event fields (the message & local event variables)
        write!(writer, "{}", cyan.prefix())?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        write!(writer, "{}", cyan.suffix())?;

        writeln!(writer)
    }
}

pub fn init() -> tracing_appender::non_blocking::WorkerGuard {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into());

    // Non-blocking file appender — appends to `kroma.log` in the working directory.
    let file_appender = tracing_appender::rolling::never(".", "kroma.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Plain file layer (no ANSI colours — they corrupt log files).
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(env_filter)
        // Coloured stdout layer
        .with(tracing_subscriber::fmt::layer().event_format(CustomLogFormat))
        // Plain file layer
        .with(file_layer)
        .init();

    guard // must be held alive for the duration of the program
}
