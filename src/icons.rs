//! Signal Desktop's icons (`images/icons/v3`, AGPL-3.0), recolored white so
//! egui's tint gives them any color.

use egui::{Color32, Image, ImageSource, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Icon {
    Archive,
    ArrowDown,
    BellSlashFill,
    ChatFill,
    Chat,
    Check,
    ChevronDown,
    ChevronLeft,
    Copy,
    Edit,
    Emoji,
    ErrorCircleCompact,
    Forward,
    Group,
    HeartPlus,
    Info,
    Lock,
    MessagestatusDelivered,
    MessagestatusRead,
    MessagestatusSending,
    MessagestatusSent,
    MicFillLight,
    MicSlashFillLight,
    Mic,
    More,
    Note,
    Person,
    PhoneCompact,
    PhoneDownFillLight,
    PhoneFill,
    Phone,
    PhotoSquare,
    PipMaximizeLight,
    Plus,
    QrCode,
    RaiseHandLight,
    Reply,
    SafetyNumber,
    SearchCompactLight,
    SendFill,
    SettingsFill,
    Settings,
    ShareScreenFillLight,
    Speaker,
    Sticker,
    StoriesFill,
    Stories,
    TimerCompact,
    Trash,
    VideoFillLight,
    VideoFill,
    VideoSlashFillLight,
    Video,
    XCompact,
    X,
}

impl Icon {
    fn source(self) -> (&'static str, &'static [u8]) {
        match self {
            Icon::Archive => ("bytes://icon/archive.svg", include_bytes!("../assets/icons/archive.svg")),
            Icon::ArrowDown => ("bytes://icon/arrow-down.svg", include_bytes!("../assets/icons/arrow-down.svg")),
            Icon::BellSlashFill => ("bytes://icon/bell-slash-fill.svg", include_bytes!("../assets/icons/bell-slash-fill.svg")),
            Icon::ChatFill => ("bytes://icon/chat-fill.svg", include_bytes!("../assets/icons/chat-fill.svg")),
            Icon::Chat => ("bytes://icon/chat.svg", include_bytes!("../assets/icons/chat.svg")),
            Icon::Check => ("bytes://icon/check.svg", include_bytes!("../assets/icons/check.svg")),
            Icon::ChevronDown => ("bytes://icon/chevron-down.svg", include_bytes!("../assets/icons/chevron-down.svg")),
            Icon::ChevronLeft => ("bytes://icon/chevron-left.svg", include_bytes!("../assets/icons/chevron-left.svg")),
            Icon::Copy => ("bytes://icon/copy.svg", include_bytes!("../assets/icons/copy.svg")),
            Icon::Edit => ("bytes://icon/edit.svg", include_bytes!("../assets/icons/edit.svg")),
            Icon::Emoji => ("bytes://icon/emoji.svg", include_bytes!("../assets/icons/emoji.svg")),
            Icon::ErrorCircleCompact => ("bytes://icon/error-circle-compact.svg", include_bytes!("../assets/icons/error-circle-compact.svg")),
            Icon::Forward => ("bytes://icon/forward.svg", include_bytes!("../assets/icons/forward.svg")),
            Icon::Group => ("bytes://icon/group.svg", include_bytes!("../assets/icons/group.svg")),
            Icon::HeartPlus => ("bytes://icon/heart-plus.svg", include_bytes!("../assets/icons/heart-plus.svg")),
            Icon::Info => ("bytes://icon/info.svg", include_bytes!("../assets/icons/info.svg")),
            Icon::Lock => ("bytes://icon/lock.svg", include_bytes!("../assets/icons/lock.svg")),
            Icon::MessagestatusDelivered => ("bytes://icon/messagestatus-delivered.svg", include_bytes!("../assets/icons/messagestatus-delivered.svg")),
            Icon::MessagestatusRead => ("bytes://icon/messagestatus-read.svg", include_bytes!("../assets/icons/messagestatus-read.svg")),
            Icon::MessagestatusSending => ("bytes://icon/messagestatus-sending.svg", include_bytes!("../assets/icons/messagestatus-sending.svg")),
            Icon::MessagestatusSent => ("bytes://icon/messagestatus-sent.svg", include_bytes!("../assets/icons/messagestatus-sent.svg")),
            Icon::MicFillLight => ("bytes://icon/mic-fill-light.svg", include_bytes!("../assets/icons/mic-fill-light.svg")),
            Icon::MicSlashFillLight => ("bytes://icon/mic-slash-fill-light.svg", include_bytes!("../assets/icons/mic-slash-fill-light.svg")),
            Icon::Mic => ("bytes://icon/mic.svg", include_bytes!("../assets/icons/mic.svg")),
            Icon::More => ("bytes://icon/more.svg", include_bytes!("../assets/icons/more.svg")),
            Icon::Note => ("bytes://icon/note.svg", include_bytes!("../assets/icons/note.svg")),
            Icon::Person => ("bytes://icon/person.svg", include_bytes!("../assets/icons/person.svg")),
            Icon::PhoneCompact => ("bytes://icon/phone-compact.svg", include_bytes!("../assets/icons/phone-compact.svg")),
            Icon::PhoneDownFillLight => ("bytes://icon/phone-down-fill-light.svg", include_bytes!("../assets/icons/phone-down-fill-light.svg")),
            Icon::PhoneFill => ("bytes://icon/phone-fill.svg", include_bytes!("../assets/icons/phone-fill.svg")),
            Icon::Phone => ("bytes://icon/phone.svg", include_bytes!("../assets/icons/phone.svg")),
            Icon::PhotoSquare => ("bytes://icon/photo-square.svg", include_bytes!("../assets/icons/photo-square.svg")),
            Icon::PipMaximizeLight => ("bytes://icon/pip-maximize-light.svg", include_bytes!("../assets/icons/pip-maximize-light.svg")),
            Icon::Plus => ("bytes://icon/plus.svg", include_bytes!("../assets/icons/plus.svg")),
            Icon::QrCode => ("bytes://icon/qr_code.svg", include_bytes!("../assets/icons/qr_code.svg")),
            Icon::RaiseHandLight => ("bytes://icon/raise_hand-light.svg", include_bytes!("../assets/icons/raise_hand-light.svg")),
            Icon::Reply => ("bytes://icon/reply.svg", include_bytes!("../assets/icons/reply.svg")),
            Icon::SafetyNumber => ("bytes://icon/safety_number.svg", include_bytes!("../assets/icons/safety_number.svg")),
            Icon::SearchCompactLight => ("bytes://icon/search-compact-light.svg", include_bytes!("../assets/icons/search-compact-light.svg")),
            Icon::SendFill => ("bytes://icon/send-fill.svg", include_bytes!("../assets/icons/send-fill.svg")),
            Icon::SettingsFill => ("bytes://icon/settings-fill.svg", include_bytes!("../assets/icons/settings-fill.svg")),
            Icon::Settings => ("bytes://icon/settings.svg", include_bytes!("../assets/icons/settings.svg")),
            Icon::ShareScreenFillLight => ("bytes://icon/share_screen-fill-light.svg", include_bytes!("../assets/icons/share_screen-fill-light.svg")),
            Icon::Speaker => ("bytes://icon/speaker.svg", include_bytes!("../assets/icons/speaker.svg")),
            Icon::Sticker => ("bytes://icon/sticker.svg", include_bytes!("../assets/icons/sticker.svg")),
            Icon::StoriesFill => ("bytes://icon/stories-fill.svg", include_bytes!("../assets/icons/stories-fill.svg")),
            Icon::Stories => ("bytes://icon/stories.svg", include_bytes!("../assets/icons/stories.svg")),
            Icon::TimerCompact => ("bytes://icon/timer-compact.svg", include_bytes!("../assets/icons/timer-compact.svg")),
            Icon::Trash => ("bytes://icon/trash.svg", include_bytes!("../assets/icons/trash.svg")),
            Icon::VideoFillLight => ("bytes://icon/video-fill-light.svg", include_bytes!("../assets/icons/video-fill-light.svg")),
            Icon::VideoFill => ("bytes://icon/video-fill.svg", include_bytes!("../assets/icons/video-fill.svg")),
            Icon::VideoSlashFillLight => ("bytes://icon/video-slash-fill-light.svg", include_bytes!("../assets/icons/video-slash-fill-light.svg")),
            Icon::Video => ("bytes://icon/video.svg", include_bytes!("../assets/icons/video.svg")),
            Icon::XCompact => ("bytes://icon/x-compact.svg", include_bytes!("../assets/icons/x-compact.svg")),
            Icon::X => ("bytes://icon/x.svg", include_bytes!("../assets/icons/x.svg")),
        }
    }

    pub fn image(self, size: f32, tint: Color32) -> Image<'static> {
        let (uri, bytes) = self.source();
        Image::new(ImageSource::Bytes { uri: uri.into(), bytes: bytes.into() })
            .fit_to_exact_size(Vec2::splat(size))
            .tint(tint)
    }

    /// Draws the icon centered in `rect`.
    pub fn paint(self, ui: &egui::Ui, rect: egui::Rect, size: f32, tint: Color32) {
        self.image(size, tint).paint_at(ui, egui::Rect::from_center_size(rect.center(), Vec2::splat(size)));
    }
}
