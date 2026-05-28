use mime::Mime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Wav,
    Flac,
    Ogg,
    Mp3,
    M4a,
    Aac,
}

impl Type {
    #[must_use]
    pub fn detect(head: &[u8; 12]) -> Option<Self> {
        if &head[..4] == b"RIFF" && &head[8..12] == b"WAVE" {
            Some(Self::Wav)
        } else if &head[..4] == b"fLaC" {
            Some(Self::Flac)
        } else if &head[..4] == b"OggS" {
            Some(Self::Ogg)
        } else if &head[..3] == b"ID3"
            || (head[..2] == [0xff, 0xfb]
                || head[..2] == [0xff, 0xf3]
                || head[..2] == [0xff, 0xf2])
        {
            Some(Self::Mp3)
        } else if &head[4..12] == b"ftypM4A " {
            Some(Self::M4a)
        } else if head[..2] == [0xff, 0xf1] || head[..2] == [0xff, 0xf9] {
            Some(Self::Aac)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn to_ext(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::Mp3 => "mp3",
            Self::M4a => "m4a",
            Self::Aac => "aac",
        }
    }

    #[expect(clippy::missing_panics_doc)]
    #[must_use]
    pub fn to_mime(self) -> Mime {
        match self {
            Self::Wav => "audio/wav",
            Self::Flac => "audio/flac",
            Self::Ogg => "application/ogg",
            Self::Mp3 => "audio/mpeg",
            Self::M4a => "audio/mp4",
            Self::Aac => "audio/aac",
        }
        .parse()
        .unwrap()
    }
}
