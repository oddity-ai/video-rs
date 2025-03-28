#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ThreadingConfig {
    Auto { kind: ThreadingKind },
    Manual { kind: ThreadingKind, count: usize },
}

impl From<ThreadingConfig> for ffmpeg_next::threading::Config {
    fn from(value: ThreadingConfig) -> Self {
        match value {
            ThreadingConfig::Auto { kind } => ffmpeg_next::threading::Config {
                count: 0,
                kind: kind.into(),
                safe: true,
            },
            ThreadingConfig::Manual { count, kind } => ffmpeg_next::threading::Config {
                count,
                kind: kind.into(),
                safe: true,
            },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ThreadingKind {
    Frame,
    Slice,
}

impl From<ThreadingKind> for ffmpeg_next::threading::Type {
    fn from(kind: ThreadingKind) -> Self {
        match kind {
            ThreadingKind::Frame => ffmpeg_next::threading::Type::Frame,
            ThreadingKind::Slice => ffmpeg_next::threading::Type::Slice,
        }
    }
}
