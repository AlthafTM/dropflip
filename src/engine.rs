use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default, Serialize)]
pub struct Engines {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub pandoc: Option<PathBuf>,
    pub typst: Option<PathBuf>,
    pub magick: Option<PathBuf>,
    pub soffice: Option<PathBuf>,
    pub pdftoppm: Option<PathBuf>,
    pub ghostscript: Option<PathBuf>,
    pub pdf_word: Option<PathBuf>,
}

impl Engines {
    pub fn discover() -> Self {
        Self {
            ffmpeg: find_tool("ffmpeg"),
            ffprobe: find_tool("ffprobe"),
            pandoc: find_tool("pandoc"),
            typst: find_tool("typst"),
            magick: find_tool("magick"),
            soffice: find_tool("soffice"),
            pdftoppm: find_tool("pdftoppm"),
            ghostscript: find_tool("gswin64c"),
            pdf_word: find_tool("pdf-word"),
        }
    }
}

fn find_tool(name: &str) -> Option<PathBuf> {
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("DROPFORM_TOOLS") {
        roots.push(PathBuf::from(root));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            roots.push(root.join("tools"));
        }
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools"));
    if let Some(paths) = std::env::var_os("PATH") {
        roots.extend(std::env::split_paths(&paths));
    }
    if cfg!(windows) {
        if let Some(programs) = std::env::var_os("ProgramFiles") {
            roots.push(PathBuf::from(programs).join("LibreOffice/program"));
        }
    }
    roots.into_iter().find_map(|root| {
        let mut choices = vec![root.join(&filename), root.join(name).join(&filename)];
        if name == "soffice" {
            choices.insert(0, root.join("office/program/soffice.com"));
            choices.push(root.join("office/program/soffice.exe"));
        }
        choices.into_iter().find(|p| p.is_file())
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Image,
    Audio,
    Video,
    Document,
    Pdf,
    Data,
    Other,
}

#[derive(Clone, Serialize, Debug)]
pub struct Format {
    pub ext: String,
    pub label: String,
    pub detail: String,
    pub compression: bool,
}

#[derive(Clone, Serialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub extension: String,
    pub bytes: u64,
    pub kind: Kind,
    pub summary: String,
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub has_audio: bool,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_codecs: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub subtitle_codecs: Vec<String>,
    pub audio_depth: u32,
    pub audio_float: bool,
    pub frames: usize,
    pub formats: Vec<Format>,
    pub note: Option<String>,
}

fn format(ext: &str, label: &str, detail: &str, compression: bool) -> Format {
    Format {
        ext: ext.into(),
        label: label.into(),
        detail: detail.into(),
        compression,
    }
}
fn doc_reader(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "md" | "markdown" | "txt" => "markdown",
        "html" | "htm" => "html",
        "docx" => "docx",
        "odt" => "odt",
        "rtf" => "rtf",
        "epub" => "epub",
        "tex" | "latex" => "latex",
        "rst" => "rst",
        "org" => "org",
        "fb2" => "fb2",
        "ipynb" => "ipynb",
        "opml" => "opml",
        "typ" => "typst",
        _ => return None,
    })
}
fn spreadsheet(ext: &str) -> bool {
    matches!(
        ext,
        "xls" | "xlsx" | "xlsm" | "xlsb" | "ods" | "xltx" | "xlt"
    )
}
fn presentation(ext: &str) -> bool {
    matches!(
        ext,
        "ppt" | "pptx" | "pptm" | "pps" | "ppsx" | "odp" | "potx"
    )
}
fn office_text(ext: &str) -> bool {
    matches!(
        ext,
        "doc" | "docx" | "docm" | "dotx" | "dotm" | "odt" | "rtf" | "txt"
    )
}

pub fn inspect(path: &Path, engines: &Engines) -> Result<FileInfo> {
    let path = path
        .canonicalize()
        .context("The dropped file is no longer available")?;
    let meta = path.metadata()?;
    if !meta.is_file() {
        bail!("Drop a file rather than a folder.");
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut kind = if matches!(
        ext.as_str(),
        "json" | "yaml" | "yml" | "toml" | "csv" | "tsv"
    ) {
        Kind::Data
    } else if ext == "pdf" {
        Kind::Pdf
    } else if doc_reader(&ext).is_some()
        || office_text(&ext)
        || spreadsheet(&ext)
        || presentation(&ext)
    {
        Kind::Document
    } else if matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "jfif"
            | "jpe"
            | "webp"
            | "bmp"
            | "tiff"
            | "tif"
            | "ico"
            | "tga"
            | "ppm"
            | "pgm"
            | "pbm"
            | "pnm"
            | "qoi"
            | "hdr"
            | "dds"
            | "avif"
            | "heic"
            | "heif"
            | "svg"
            | "psd"
            | "jxl"
    ) {
        Kind::Image
    } else if matches!(
        ext.as_str(),
        "mp3"
            | "wav"
            | "flac"
            | "ogg"
            | "opus"
            | "m4a"
            | "aac"
            | "aiff"
            | "aif"
            | "wma"
            | "ape"
            | "alac"
            | "amr"
            | "mka"
            | "ac3"
            | "aifc"
    ) {
        Kind::Audio
    } else if matches!(
        ext.as_str(),
        "mp4"
            | "mkv"
            | "mov"
            | "webm"
            | "avi"
            | "m4v"
            | "wmv"
            | "flv"
            | "ts"
            | "mts"
            | "m2ts"
            | "mpeg"
            | "mpg"
            | "vob"
            | "gif"
            | "3gp"
            | "3g2"
            | "m2v"
            | "ogv"
    ) {
        Kind::Video
    } else {
        Kind::Other
    };
    let guessed = image::ImageReader::open(&path)?
        .with_guessed_format()?
        .format();
    let animated_webp = if ext == "webp" {
        image::codecs::webp::WebPDecoder::new(BufReader::new(File::open(&path)?))
            .map(|d| d.has_animation())
            .unwrap_or(false)
    } else {
        false
    };
    if guessed.is_some() && ext != "gif" {
        kind = Kind::Image;
    }
    if animated_webp {
        kind = Kind::Video;
    }
    let mut info = FileInfo {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path,
        extension: ext,
        bytes: meta.len(),
        kind,
        summary: String::new(),
        duration: None,
        width: None,
        height: None,
        has_audio: false,
        video_codec: None,
        audio_codec: None,
        video_codecs: Vec::new(),
        audio_codecs: Vec::new(),
        subtitle_codecs: Vec::new(),
        audio_depth: 16,
        audio_float: false,
        frames: 1,
        formats: Vec::new(),
        note: None,
    };
    if info.kind == Kind::Image {
        if let Ok((w, h)) = image::ImageReader::open(&info.path)?
            .with_guessed_format()?
            .into_dimensions()
        {
            info.width = Some(w);
            info.height = Some(h);
        }
        if matches!(info.extension.as_str(), "tiff" | "tif") {
            let mut decoder = tiff::decoder::Decoder::new(BufReader::new(File::open(&info.path)?))?;
            while decoder.more_images() {
                decoder.next_image()?;
                info.frames += 1;
            }
        }
        let supported = guessed.is_some()
            || engines.magick.is_some()
            || engines.ffmpeg.is_some()
            || info.extension == "svg" && engines.pdf_word.is_some();
        if info.frames > 1 {
            if engines.pdf_word.is_some() {
                info.formats.extend([
                    format("pdf", "PDF", "All TIFF pages in one document", false),
                    format(
                        "png.zip",
                        "PNG pages",
                        "All pages as lossless images in a ZIP",
                        false,
                    ),
                    format(
                        "jpg.zip",
                        "JPEG pages",
                        "All pages as images in a ZIP",
                        true,
                    ),
                ]);
            }
            info.note =
                Some("Every page is preserved; image collections are saved as ZIP files.".into());
        } else if supported {
            info.formats.extend([
                format("png", "PNG", "Lossless · keeps transparency", true),
                format("jpg", "JPEG", "Photographs · white background", true),
                format(
                    "webp",
                    "WebP",
                    "Compact images · keeps transparency",
                    engines.ffmpeg.is_some(),
                ),
                format("tiff", "TIFF", "Lossless bitmap", false),
                format("bmp", "BMP", "Uncompressed bitmap", false),
                format("ico", "ICO", "Windows icon · up to 256 px", false),
                format("gif", "GIF", "Still image · indexed colors", false),
                format("tga", "TGA", "Graphics and textures", false),
                format("qoi", "QOI", "Fast lossless image", false),
                format(
                    "pdf",
                    "PDF",
                    "Image on a page; SVG graphics stay vector",
                    false,
                ),
            ]);
            if engines.pdf_word.is_some() {
                info.formats.insert(
                    3,
                    format(
                        "avif",
                        "AVIF",
                        "Modern compact image · keeps transparency",
                        true,
                    ),
                );
            }
            if guessed == Some(image::ImageFormat::Hdr) {
                info.formats.retain(|f| f.ext == "tiff");
                info.note = Some(
                    "Floating-point HDR is preserved in TIFF. SDR tone mapping is not offered."
                        .into(),
                );
            }
        }
    }
    if matches!(info.kind, Kind::Audio | Kind::Video) {
        if animated_webp && engines.pdf_word.is_some() {
            let metadata = tempfile::Builder::new().suffix(".json").tempfile()?;
            let mut command = Command::new(engines.pdf_word.as_ref().unwrap());
            command.arg(&info.path).arg(metadata.path());
            run_capture(&mut command, Duration::from_secs(20), 1024 * 1024)?;
            let value: serde_json::Value = serde_json::from_slice(&fs::read(metadata.path())?)?;
            info.width = value["width"].as_u64().map(|n| n as u32);
            info.height = value["height"].as_u64().map(|n| n as u32);
            info.frames = value["frames"].as_u64().unwrap_or(1) as usize;
            info.duration = value["duration"].as_f64();
            info.video_codec = Some("webp".into());
            info.video_codecs.push("webp".into());
        } else if let Some(probe) = &engines.ffprobe {
            let mut cmd = Command::new(probe);
            cmd.args([
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
            ])
            .arg(&info.path);
            let output = run_capture(&mut cmd, Duration::from_secs(20), 2 * 1024 * 1024)?;
            let value: serde_json::Value = serde_json::from_slice(&output)?;
            let streams = value["streams"]
                .as_array()
                .ok_or_else(|| anyhow!("No readable media streams found"))?;
            let videos = streams
                .iter()
                .filter(|s| {
                    s["codec_type"] == "video"
                        && s["disposition"]["attached_pic"].as_i64().unwrap_or(0) == 0
                })
                .collect::<Vec<_>>();
            let audios = streams
                .iter()
                .filter(|s| s["codec_type"] == "audio")
                .collect::<Vec<_>>();
            if videos.is_empty() && audios.is_empty() {
                bail!("This file has no convertible video or audio stream.");
            }
            info.kind = if videos.is_empty() {
                Kind::Audio
            } else {
                Kind::Video
            };
            info.has_audio = !audios.is_empty();
            info.video_codecs = videos
                .iter()
                .map(|s| s["codec_name"].as_str().unwrap_or("").into())
                .collect();
            info.audio_codecs = audios
                .iter()
                .map(|s| s["codec_name"].as_str().unwrap_or("").into())
                .collect();
            info.subtitle_codecs = streams
                .iter()
                .filter(|s| s["codec_type"] == "subtitle")
                .map(|s| s["codec_name"].as_str().unwrap_or("").into())
                .collect();
            info.video_codec = info.video_codecs.first().cloned();
            info.audio_codec = info.audio_codecs.first().cloned();
            if let Some(video) = videos.first() {
                info.width = video["width"].as_u64().map(|v| v as u32);
                info.height = video["height"].as_u64().map(|v| v as u32);
            }
            if let Some(audio) = audios.first() {
                let sample = audio["sample_fmt"].as_str().unwrap_or("");
                info.audio_float = sample.starts_with("flt") || sample.starts_with("dbl");
                info.audio_depth = audio["bits_per_raw_sample"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
                    .filter(|v: &u32| *v > 0)
                    .or_else(|| {
                        audio["bits_per_sample"]
                            .as_u64()
                            .map(|v| v as u32)
                            .filter(|v| *v > 0)
                    })
                    .unwrap_or(if sample.contains("32") {
                        32
                    } else if sample.contains("64") || sample.starts_with("dbl") {
                        64
                    } else {
                        16
                    });
            }
            info.duration = value["format"]["duration"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0);
        }
        if engines.ffmpeg.is_some() && engines.ffprobe.is_some() {
            if info.kind == Kind::Video {
                let text_subs = info.subtitle_codecs.iter().all(|c| {
                    matches!(
                        c.as_str(),
                        "subrip" | "ass" | "ssa" | "mov_text" | "webvtt" | "text"
                    )
                });
                if text_subs {
                    info.formats.push(format(
                        "mp4",
                        "MP4",
                        "H.264 · all audio and compatible text subtitles",
                        true,
                    ));
                }
                info.formats.push(format(
                    "mkv",
                    "MKV",
                    "All video, audio, subtitles and attachments",
                    true,
                ));
                if text_subs {
                    info.formats.extend([
                        format("webm", "WebM", "VP9 · all audio and text subtitles", true),
                        format("mov", "MOV", "H.264 · all audio and text subtitles", true),
                    ]);
                }
                if info.subtitle_codecs.is_empty() {
                    info.formats
                        .push(format("avi", "AVI", "MPEG-4 video with MP3 audio", true));
                }
                info.formats
                    .push(format("gif", "GIF", "Animated image · no audio", true));
            }
            if info.kind == Kind::Audio || info.has_audio {
                info.formats.extend([
                    format(
                        "mp3",
                        "MP3",
                        "First audio track · broad compatibility",
                        true,
                    ),
                    format("m4a", "M4A", "AAC audio", true),
                    format("aac", "AAC", "AAC elementary audio stream", true),
                    format("opus", "Opus", "Efficient voice and music audio", true),
                    format("ogg", "OGG", "Vorbis audio", true),
                    format("flac", "FLAC", "Lossless integer audio", true),
                    format("wav", "WAV", "Uncompressed · keeps source bit depth", false),
                    format(
                        "aiff",
                        "AIFF",
                        "Uncompressed · keeps source bit depth",
                        false,
                    ),
                    format("wma", "WMA", "Windows Media Audio", true),
                ]);
            }
            if info.kind == Kind::Video {
                info.note=Some("Containers preserve supported tracks. Audio/GIF exports intentionally select the first track. Bitmap subtitles require MKV.".into());
            }
        }
    }
    if info.kind == Kind::Document {
        if spreadsheet(&info.extension) {
            if info.bytes > 64 * 1024 * 1024 {
                bail!("Spreadsheet inspection is limited to 64 MB.");
            }
            let sheets = crate::sheets::names(&info.path)?;
            info.summary = format!("{} sheets", sheets.len());
            if sheets.len() == 1 {
                info.formats.extend([
                    format(
                        "csv",
                        "CSV",
                        "Sheet values · no formulas or formatting",
                        false,
                    ),
                    format("tsv", "TSV", "Sheet values as tab-separated text", false),
                ]);
            } else {
                info.formats.extend([
                    format(
                        "csv.zip",
                        "CSV sheets",
                        "Every sheet as a separate CSV in a ZIP",
                        false,
                    ),
                    format(
                        "tsv.zip",
                        "TSV sheets",
                        "Every sheet as a separate TSV in a ZIP",
                        false,
                    ),
                ]);
            }
            info.formats.push(format(
                "json",
                "JSON",
                "All sheets as typed row arrays",
                false,
            ));
            if engines.soffice.is_some() {
                info.formats.extend([
                    format(
                        "pdf",
                        "PDF",
                        "All printable sheets rendered by LibreOffice",
                        false,
                    ),
                    format(
                        "xlsx",
                        "Excel",
                        "Workbook with sheets, formulas and formatting",
                        false,
                    ),
                    format("xls", "Excel 97–2003", "Legacy workbook format", false),
                    format("ods", "OpenDocument sheet", "Editable workbook", false),
                ]);
            }
            info.note=Some("Text exports keep every sheet and cached values. Use workbook formats to retain formulas and formatting.".into());
        } else if presentation(&info.extension) {
            if engines.soffice.is_some() {
                info.formats.extend([
                    format("pdf", "PDF", "All slides rendered by LibreOffice", false),
                    format("pptx", "PowerPoint", "Editable slides", false),
                    format("ppt", "PowerPoint 97–2003", "Legacy editable slides", false),
                    format("odp", "OpenDocument slides", "Editable slides", false),
                ]);
            }
        } else {
            if engines.pandoc.is_some() && doc_reader(&info.extension).is_some() {
                info.formats.extend([
                    format("docx", "Word", "Editable document", false),
                    format(
                        "html",
                        "HTML",
                        "Web document with embedded resources",
                        false,
                    ),
                    format(
                        "md",
                        "Markdown",
                        "Text formatting; extracted assets saved together",
                        false,
                    ),
                    format("txt", "Text", "All document text", false),
                    format("epub", "EPUB", "Reflowable ebook", false),
                    format("odt", "OpenDocument", "Editable document", false),
                    format("rtf", "RTF", "Rich text", false),
                    format("tex", "LaTeX", "Typesetting source", false),
                    format(
                        "rst",
                        "reStructuredText",
                        "Technical document source",
                        false,
                    ),
                ]);
                if engines.typst.is_some() {
                    info.formats
                        .insert(0, format("pdf", "PDF", "Typeset document", false));
                }
            }
            if engines.soffice.is_some() && office_text(&info.extension) {
                for option in [
                    format("pdf", "PDF", "Layout rendered by LibreOffice", false),
                    format("docx", "Word", "Editable document", false),
                    format("doc", "Word 97–2003", "Legacy editable document", false),
                    format("odt", "OpenDocument", "Editable document", false),
                    format("rtf", "RTF", "Rich text", false),
                    format("txt", "Text", "All document text", false),
                ] {
                    if !info.formats.iter().any(|f| f.ext == option.ext) {
                        info.formats.push(option);
                    }
                }
            }
            info.note=Some("Office document formats use LibreOffice for layout. Reflowable formats cannot retain exact page layout.".into());
        }
    }
    if info.kind == Kind::Document
        && !spreadsheet(&info.extension)
        && !presentation(&info.extension)
    {
        for option in &mut info.formats {
            if matches!(option.ext.as_str(), "md" | "tex" | "rst") {
                option.ext.push_str(".zip");
                option.label.push_str(" bundle");
                option.detail = "Document source and referenced images together in a ZIP".into();
            }
        }
    }
    if info.kind == Kind::Pdf {
        let document = lopdf::Document::load(&info.path).context("Could not open this PDF")?;
        if document.is_encrypted() {
            bail!("This PDF is encrypted. Save an unlocked copy first.");
        }
        info.frames = document.get_pages().len();
        if info.frames == 0 {
            bail!("This PDF has no pages.");
        }
        info.summary = format!("{} pages", info.frames);
        info.formats.push(format(
            "pdf",
            "PDF",
            "Compress streams and supported embedded JPEG images",
            true,
        ));
        info.formats.push(format(
            "txt",
            "Text",
            if engines.pdf_word.is_some() {
                "All pages · OCR on scanned pages"
            } else {
                "Selectable text · no OCR"
            },
            false,
        ));
        if engines.pdf_word.is_some() {
            info.formats.insert(
                0,
                format(
                    "docx",
                    "Word",
                    "Layout, tables and images; English/Indonesian OCR",
                    false,
                ),
            );
            info.formats.extend([
                format(
                    "xlsx",
                    "Excel tables",
                    "Detected digital tables across all pages; fails if none are found",
                    false,
                ),
                format(
                    "pptx",
                    "PowerPoint",
                    "All pages; positioned digital text over preserved artwork",
                    false,
                ),
            ]);
            if engines.soffice.is_some() {
                info.formats.push(format(
                    "ppt",
                    "PowerPoint 97–2003",
                    "All pages as legacy slides",
                    false,
                ));
            }
            if info.frames == 1 {
                info.formats.extend([
                    format("png", "PNG", "Page rendered at 180 dpi", false),
                    format("jpg", "JPEG", "Page rendered at 180 dpi", true),
                    format("svg", "SVG", "Scalable page graphics", false),
                ]);
            } else {
                info.formats.extend([
                    format(
                        "png.zip",
                        "PNG pages",
                        "Every page rendered in a ZIP",
                        false,
                    ),
                    format(
                        "jpg.zip",
                        "JPEG pages",
                        "Every page rendered in a ZIP",
                        true,
                    ),
                    format(
                        "svg.zip",
                        "SVG pages",
                        "Every page as vector graphics in a ZIP",
                        false,
                    ),
                ]);
            }
        } else if engines.pdftoppm.is_some() && info.frames == 1 {
            info.formats.extend([
                format("png", "PNG", "Page at 150 dpi", false),
                format("jpg", "JPEG", "Page at 150 dpi", true),
            ]);
        }
        info.note=Some("PDF-to-Word reconstructs layout and may reflow complex pages. Excel extracts detected tables, not arbitrary page artwork. Scanned slides remain pictures.".into());
    }
    if info.kind == Kind::Data {
        info.formats.extend([
            format("json", "JSON", "Structured data", true),
            format("yaml", "YAML", "Readable structured data", false),
            format("toml", "TOML", "Table-based configuration", false),
            format("csv", "CSV", "Flat records as a table", false),
            format("tsv", "TSV", "Tab-separated flat records", false),
            format(
                "xlsx",
                "Excel",
                "Typed records; CSV identifiers stay text",
                false,
            ),
        ]);
        if engines.soffice.is_some() {
            info.formats.extend([
                format(
                    "ods",
                    "OpenDocument sheet",
                    "Flat records as a workbook",
                    false,
                ),
                format(
                    "pdf",
                    "PDF",
                    "Flat table rendered as printable sheets",
                    false,
                ),
            ]);
        }
        info.note=Some("Tabular outputs require flat records. Unsupported nested data fails without publishing partial files.".into());
        if matches!(info.extension.as_str(), "csv" | "tsv") {
            info.formats.retain(|f| f.ext != "toml");
        } else {
            if info.bytes > 64 * 1024 * 1024 {
                bail!("Structured-data conversion is limited to 64 MB.");
            }
            let value = read_structured(&info.path, &info.extension)?;
            let table = value.as_array().is_some_and(|rows| {
                rows.iter().all(|row| {
                    row.as_object().is_some_and(|cells| {
                        cells
                            .values()
                            .all(|value| !value.is_object() && !value.is_array())
                    })
                })
            });
            let toml = value.is_object() && toml_compatible(&value);
            info.formats.retain(|f| match f.ext.as_str() {
                "toml" => toml,
                "csv" | "tsv" | "xlsx" | "ods" | "pdf" => table,
                _ => true,
            });
        }
    }
    if info.summary.is_empty() {
        info.summary = match (info.width, info.height, info.duration) {
            (Some(w), Some(h), Some(d)) => format!("{w} × {h} · {}", duration(d)),
            (Some(w), Some(h), _) => format!("{w} × {h}"),
            (_, _, Some(d)) => duration(d),
            _ => format!(
                "{} file",
                if info.extension.is_empty() {
                    "Unknown".into()
                } else {
                    info.extension.to_uppercase()
                }
            ),
        };
    }
    info.formats
        .retain(|f| f.ext != info.extension || f.compression);
    info.formats.push(format(
        "zip",
        "ZIP archive",
        "Archive a copy of the original file",
        true,
    ));
    if info.kind == Kind::Other {
        info.note =
            Some("No meaningful converter is available. ZIP archives the original file.".into());
    }
    Ok(info)
}
pub fn duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
pub fn size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn load_image(path: &Path) -> Result<image::DynamicImage> {
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(512 * 1024 * 1024);
    limits.max_image_width = Some(24000);
    limits.max_image_height = Some(24000);
    reader.limits(limits);
    use image::ImageDecoder;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image)
}

#[derive(Clone)]
pub struct Request {
    pub info: FileInfo,
    pub format: String,
    pub compress: bool,
    pub compression: u8,
    pub destination: PathBuf,
}
#[derive(Clone, Serialize)]
pub struct Outcome {
    pub path: PathBuf,
    pub bytes: u64,
    pub original_bytes: u64,
    pub elapsed: f64,
    pub message: String,
}
#[derive(Clone)]
pub struct Progress {
    pub fraction: Option<f32>,
    pub message: String,
}
type Notify<'a> = &'a mut dyn FnMut(Progress);

fn notify(callback: Notify<'_>, fraction: Option<f32>, message: &str) {
    callback(Progress {
        fraction,
        message: message.into(),
    });
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("Conversion canceled");
    }
    Ok(())
}

pub fn convert(
    request: &Request,
    engines: &Engines,
    cancel: &Arc<AtomicBool>,
    callback: Notify<'_>,
) -> Result<Outcome> {
    let started = Instant::now();
    check_cancel(cancel)?;
    if !request.info.formats.iter().any(|f| f.ext == request.format) {
        bail!("This output format is not supported for the selected file.");
    }
    fs::create_dir_all(&request.destination).context("Could not create the output folder")?;
    let dir = request.destination.canonicalize()?;
    let temporary = tempfile::Builder::new()
        .prefix(".dropform-")
        .suffix(&format!(".{}", request.format))
        .tempfile_in(&dir)?;
    let temp = temporary.path();
    notify(callback, None, "Preparing conversion");
    let mut message = "Saved to your folder".to_string();
    if request.format == "zip" {
        zip_file(
            &request.info.path,
            temp,
            if request.compress {
                request.compression
            } else {
                60
            },
            cancel,
            callback,
        )?;
    } else {
        if !request.compress && request.format == request.info.extension {
            fs::copy(&request.info.path, temp)?;
        } else {
            match request.info.kind {
                Kind::Audio | Kind::Video => media(request, temp, engines, cancel, callback)?,
                Kind::Image => convert_image(request, temp, engines, cancel, callback)?,
                Kind::Document => document(request, temp, engines, cancel, callback)?,
                Kind::Pdf => pdf(request, temp, engines, cancel, callback)?,
                Kind::Data => {
                    if matches!(request.format.as_str(), "xlsx" | "ods" | "pdf") {
                        let staging = tempfile::tempdir()?;
                        let workbook = staging.path().join("table.xlsx");
                        if matches!(request.info.extension.as_str(), "csv" | "tsv") {
                            crate::sheets::delimited_to_excel(
                                &request.info.path,
                                &workbook,
                                if request.info.extension == "tsv" {
                                    b'\t'
                                } else {
                                    b','
                                },
                                cancel,
                            )?;
                        } else {
                            data(request, &workbook, Some(cancel))?;
                        }
                        if request.format == "xlsx" {
                            fs::copy(workbook, temp)?;
                        } else {
                            office_document(
                                &workbook,
                                &request.format,
                                temp,
                                engines,
                                cancel,
                                callback,
                            )?;
                        }
                    } else {
                        data(request, temp, Some(cancel))?;
                    }
                }
                Kind::Other => bail!("No converter is available for this file"),
            }
        }
    }
    check_cancel(cancel)?;
    let mut bytes = temp.metadata()?.len();
    if bytes == 0 {
        bail!("The converter produced an empty file. The original is unchanged.");
    }
    if request.compress && request.format == request.info.extension && bytes >= request.info.bytes {
        fs::copy(&request.info.path, temp)?;
        bytes = request.info.bytes;
        message = "Already compact. Saved a copy at the original quality.".into();
    }
    notify(callback, Some(0.99), "Saving finished file");
    let stem = request
        .info
        .path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let suffix = if request.compress && request.format == request.info.extension {
        "-compressed"
    } else {
        ""
    };
    // persist_noclobber is atomic: a second job cannot overwrite the chosen name.
    let mut staged = temporary;
    let path = loop {
        let mut chosen = None;
        for index in 0..10000 {
            let name = if index == 0 {
                format!("{stem}{suffix}.{}", request.format)
            } else {
                format!("{stem}{suffix} ({index}).{}", request.format)
            };
            let candidate = dir.join(name);
            if !candidate.exists() {
                chosen = Some(candidate);
                break;
            }
        }
        let candidate =
            chosen.ok_or_else(|| anyhow!("Could not find an unused output filename"))?;
        match staged.persist_noclobber(&candidate) {
            Ok(_) => break candidate,
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => staged = e.file,
            Err(e) => return Err(e.error.into()),
        }
    };
    Ok(Outcome {
        path,
        bytes,
        original_bytes: request.info.bytes,
        elapsed: started.elapsed().as_secs_f64(),
        message,
    })
}

fn media(
    r: &Request,
    out: &Path,
    e: &Engines,
    cancel: &Arc<AtomicBool>,
    cb: Notify<'_>,
) -> Result<()> {
    let strength = if r.compress {
        r.compression.clamp(1, 100) as f32 / 100.0
    } else {
        0.0
    };
    let audio_bitrate = format!("{}k", (256.0 - 192.0 * strength).round() as u32);
    let opus_bitrate = format!("{}k", (192.0 - 152.0 * strength).round() as u32);
    let mp3_quality = (9.0 * strength).round().to_string();
    let vorbis_quality = (6.0 - 5.0 * strength).round().to_string();
    let flac_level = if r.compress {
        (1.0 + 7.0 * strength).round().to_string()
    } else {
        "5".into()
    };
    let h264_crf = (20.0 + 22.0 * strength).round().to_string();
    let vp9_crf = (30.0 + 20.0 * strength).round().to_string();
    let exe = e
        .ffmpeg
        .as_ref()
        .ok_or_else(|| anyhow!("FFmpeg is not installed"))?;
    let mut cmd = Command::new(exe);
    let staging = tempfile::tempdir()?;
    let animated = staging.path().join("frames.apng");
    let input = if r.info.extension == "webp" && r.info.kind == Kind::Video {
        auxiliary(&r.info.path, &animated, e, cancel, cb, 100)?;
        animated.as_path()
    } else {
        r.info.path.as_path()
    };
    cmd.args(["-hide_banner", "-nostdin", "-y", "-threads", "2", "-i"])
        .arg(input);
    cmd.args(["-map_metadata", "0", "-map_chapters", "0"]);
    let audio_only = matches!(
        r.format.as_str(),
        "mp3" | "m4a" | "aac" | "opus" | "ogg" | "flac" | "wav" | "aiff" | "wma"
    );
    if audio_only {
        cmd.args(["-map", "0:a:0"]);
        if r.info.kind == Kind::Audio && matches!(r.format.as_str(), "mp3" | "m4a" | "flac") {
            cmd.args([
                "-map",
                "0:v?",
                "-c:v",
                "copy",
                "-disposition:v",
                "attached_pic",
            ]);
        } else {
            cmd.arg("-vn");
        }
        match r.format.as_str() {
            "mp3" => {
                cmd.args(["-c:a", "libmp3lame", "-q:a", &mp3_quality]);
            }
            "m4a" | "aac" => {
                cmd.args(["-c:a", "aac", "-b:a", &audio_bitrate]);
            }
            "opus" => {
                cmd.args(["-c:a", "libopus", "-b:a", &opus_bitrate]);
            }
            "ogg" => {
                cmd.args(["-c:a", "libvorbis", "-q:a", &vorbis_quality]);
            }
            "flac" => {
                cmd.args(["-c:a", "flac", "-compression_level", &flac_level]);
                if r.info.audio_depth > 16 || r.info.audio_float {
                    cmd.args(["-sample_fmt", "s32", "-bits_per_raw_sample", "24"]);
                }
            }
            "wav" => {
                cmd.args(["-c:a", pcm_codec(&r.info, false)]);
            }
            "aiff" => {
                cmd.args(["-c:a", pcm_codec(&r.info, true)]);
            }
            "wma" => {
                cmd.args(["-c:a", "wmav2", "-b:a", &audio_bitrate]);
            }
            _ => {}
        }
    } else if r.format == "gif" {
        let fps = (15.0 - 10.0 * strength).round() as u32;
        let width = (720.0 - 400.0 * strength).round() as u32;
        let colors = (256.0 - 192.0 * strength).round() as u32;
        let prepare = if r.compress {
            format!("fps={fps},scale='min({width},iw)':-1:flags=lanczos,")
        } else {
            String::new()
        };
        let filter = format!(
            "[0:v:0]{prepare}split[a][b];[a]palettegen=max_colors={colors}[p];[b][p]paletteuse"
        );
        cmd.args(["-filter_complex", &filter, "-an", "-loop", "0"]);
    } else {
        cmd.args(["-map", "0:V", "-map", "0:a?"]);
        if !r.info.subtitle_codecs.is_empty() {
            cmd.args([
                "-map",
                "0:s?",
                "-c:s",
                match r.format.as_str() {
                    "mkv" => "copy",
                    "webm" => "webvtt",
                    _ => "mov_text",
                },
            ]);
        }
        if r.format == "mkv" {
            cmd.args(["-map", "0:t?", "-c:t", "copy"]);
        }
        let copy = !r.compress
            && r.format != r.info.extension
            && match r.format.as_str() {
                "mkv" => true,
                "mp4" | "mov" => {
                    r.info
                        .video_codecs
                        .iter()
                        .all(|v| matches!(v.as_str(), "h264" | "hevc"))
                        && r.info
                            .audio_codecs
                            .iter()
                            .all(|a| matches!(a.as_str(), "aac" | "mp3" | "alac"))
                }
                "webm" => {
                    r.info
                        .video_codecs
                        .iter()
                        .all(|v| matches!(v.as_str(), "vp8" | "vp9" | "av1"))
                        && r.info
                            .audio_codecs
                            .iter()
                            .all(|a| matches!(a.as_str(), "opus" | "vorbis"))
                }
                _ => false,
            };
        if copy {
            cmd.args(["-c:v", "copy", "-c:a", "copy"]);
            notify(cb, None, "Copying compatible streams without re-encoding");
        } else if r.format == "avi" {
            cmd.args([
                "-c:v",
                "mpeg4",
                "-q:v",
                &(3.0 + 20.0 * strength).round().to_string(),
                "-c:a",
                "libmp3lame",
                "-q:a",
                &mp3_quality,
                "-vf",
                "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                "-pix_fmt",
                "yuv420p",
                "-threads",
                "2",
            ]);
        } else if r.format == "webm" {
            cmd.args([
                "-c:v",
                "libvpx-vp9",
                "-crf",
                &vp9_crf,
                "-b:v",
                "0",
                "-deadline",
                "realtime",
                "-cpu-used",
                "5",
                "-row-mt",
                "1",
                "-vf",
                "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "libopus",
                "-b:a",
                &opus_bitrate,
                "-threads",
                "2",
            ]);
        } else {
            cmd.args([
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                &h264_crf,
                "-vf",
                "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                &audio_bitrate,
                "-threads",
                "2",
            ]);
        }
        if matches!(r.format.as_str(), "mp4" | "mov") {
            cmd.args(["-movflags", "+faststart"]);
        }
    }
    cmd.args(["-progress", "pipe:1", "-nostats"]).arg(out);
    run_process(&mut cmd, cancel, cb, r.info.duration, "Converting media")
}

fn convert_image(
    r: &Request,
    out: &Path,
    e: &Engines,
    cancel: &Arc<AtomicBool>,
    cb: Notify<'_>,
) -> Result<()> {
    check_cancel(cancel)?;
    if r.info.frames > 1
        || r.format == "avif"
        || r.info.extension == "avif"
        || r.info.extension == "svg" && r.format == "pdf"
    {
        return auxiliary(
            &r.info.path,
            out,
            e,
            cancel,
            cb,
            if r.compress { image_quality(r) } else { 100 },
        );
    }
    notify(cb, Some(0.15), "Decoding image");
    let mut source = match load_image(&r.info.path) {
        Ok(image) => image,
        Err(error) => {
            let temp = tempfile::Builder::new().suffix(".png").tempfile()?;
            if r.info.extension == "svg" && e.pdf_word.is_some() {
                auxiliary(&r.info.path, temp.path(), e, cancel, cb, 100)?;
                load_image(temp.path())?
            } else {
                let mut cmd = if let Some(magick) = &e.magick {
                    let mut c = Command::new(magick);
                    c.arg(&r.info.path).arg(temp.path());
                    c
                } else if let Some(ffmpeg) = &e.ffmpeg {
                    let mut c = Command::new(ffmpeg);
                    c.args(["-v", "error", "-nostdin", "-y", "-i"])
                        .arg(&r.info.path)
                        .args(["-frames:v", "1"])
                        .arg(temp.path());
                    c
                } else {
                    return Err(error.context("This image format needs an additional decoder"));
                };
                run_process(&mut cmd, cancel, cb, None, "Decoding image")?;
                load_image(temp.path())?
            }
        }
    };
    check_cancel(cancel)?;
    notify(cb, Some(0.55), "Writing image");
    if r.format == "ico" {
        source = source.thumbnail(256, 256);
    }
    if r.format == "pdf" {
        image_pdf(&source, out)?;
    } else if r.format == "jpg" {
        let rgb = flatten_white(&source);
        image::codecs::jpeg::JpegEncoder::new_with_quality(File::create(out)?, image_quality(r))
            .encode_image(&rgb)?;
    } else if r.format == "png" {
        use image::ImageEncoder;
        image::codecs::png::PngEncoder::new_with_quality(
            File::create(out)?,
            if r.compress && r.compression >= 65 {
                image::codecs::png::CompressionType::Best
            } else if r.compress && r.compression >= 25 {
                image::codecs::png::CompressionType::Default
            } else {
                image::codecs::png::CompressionType::Fast
            },
            image::codecs::png::FilterType::Adaptive,
        )
        .write_image(
            source.as_bytes(),
            source.width(),
            source.height(),
            source.color().into(),
        )?;
    } else if r.format == "webp" && r.compress && e.ffmpeg.is_some() {
        let intermediate = tempfile::Builder::new().suffix(".png").tempfile()?;
        source.save_with_format(intermediate.path(), image::ImageFormat::Png)?;
        drop(source);
        let mut cmd = Command::new(e.ffmpeg.as_ref().unwrap());
        let quality = image_quality(r).to_string();
        cmd.args(["-v", "error", "-nostdin", "-y", "-i"])
            .arg(intermediate.path())
            .args([
                "-c:v",
                "libwebp",
                "-quality",
                &quality,
                "-threads",
                "2",
                "-frames:v",
                "1",
            ])
            .arg(out);
        run_process(&mut cmd, cancel, cb, None, "Compressing WebP")?;
    } else {
        if matches!(r.format.as_str(), "webp" | "bmp" | "gif" | "tga" | "ico")
            && source.color().bits_per_pixel() / source.color().channel_count() as u16 > 8
        {
            source = image::DynamicImage::ImageRgba8(source.to_rgba8());
        }
        source.save_with_format(
            out,
            image::ImageFormat::from_extension(&r.format)
                .ok_or_else(|| anyhow!("Unsupported image output"))?,
        )?;
    }
    check_cancel(cancel)
}

fn image_quality(r: &Request) -> u8 {
    if r.compress {
        100 - r.compression.clamp(1, 99)
    } else {
        100
    }
}

fn auxiliary(
    input: &Path,
    out: &Path,
    e: &Engines,
    cancel: &Arc<AtomicBool>,
    cb: Notify<'_>,
    quality: u8,
) -> Result<()> {
    let executable = e
        .pdf_word
        .as_ref()
        .ok_or_else(|| anyhow!("The local image/PDF engine is missing"))?;
    let mut command = Command::new(executable);
    command
        .arg(input)
        .arg(out)
        .arg("--quality")
        .arg(quality.to_string());
    run_process(
        &mut command,
        cancel,
        cb,
        None,
        "Converting pages and images",
    )
}

fn pcm_codec(info: &FileInfo, big_endian: bool) -> &'static str {
    match (info.audio_float, info.audio_depth, big_endian) {
        (true, 64.., false) => "pcm_f64le",
        (true, 64.., true) => "pcm_f64be",
        (true, _, false) => "pcm_f32le",
        (true, _, true) => "pcm_f32be",
        (false, 25.., false) => "pcm_s32le",
        (false, 25.., true) => "pcm_s32be",
        (false, 17..=24, false) => "pcm_s24le",
        (false, 17..=24, true) => "pcm_s24be",
        (_, _, false) => "pcm_s16le",
        (_, _, true) => "pcm_s16be",
    }
}

fn zip_directory(directory: &Path, out: &Path, cancel: &AtomicBool) -> Result<()> {
    let mut archive = zip::ZipWriter::new(File::create(out)?);
    let mut pending = vec![directory.to_path_buf()];
    let mut buffer = vec![0u8; 128 * 1024];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(folder)? {
            check_cancel(cancel)?;
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path
                .strip_prefix(directory)?
                .to_string_lossy()
                .replace('\\', "/");
            archive.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )?;
            let mut input = File::open(path)?;
            loop {
                check_cancel(cancel)?;
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                archive.write_all(&buffer[..count])?;
            }
        }
    }
    archive.finish()?;
    Ok(())
}

fn flatten_white(source: &image::DynamicImage) -> image::RgbImage {
    let rgba = source.to_rgba8();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y);
        let a = p[3] as u32;
        image::Rgb([
            (p[0] as u32 * a / 255 + 255 - a) as u8,
            (p[1] as u32 * a / 255 + 255 - a) as u8,
            (p[2] as u32 * a / 255 + 255 - a) as u8,
        ])
    })
}

fn image_pdf(source: &image::DynamicImage, out: &Path) -> Result<()> {
    use lopdf::{dictionary, Document, Object, Stream};
    let rgb = flatten_white(source);
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let image=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>rgb.width() as i64,"Height"=>rgb.height() as i64,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8},rgb.as_raw().clone()));
    let w = rgb.width() as f32 * 0.75;
    let h = rgb.height() as f32 * 0.75;
    let content = doc.add_object(Stream::new(
        dictionary! {},
        format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q").into_bytes(),
    ));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![Object::Integer(0),Object::Integer(0),Object::Real(w),Object::Real(h)],"Resources"=>dictionary!{"XObject"=>dictionary!{"Im0"=>image}},"Contents"=>content});
    doc.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![Object::Reference(page)],"Count"=>1}.into(),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", catalog);
    doc.compress();
    doc.save(out)?;
    Ok(())
}

fn document(
    r: &Request,
    out: &Path,
    e: &Engines,
    cancel: &Arc<AtomicBool>,
    cb: Notify<'_>,
) -> Result<()> {
    if spreadsheet(&r.info.extension)
        && matches!(
            r.format.as_str(),
            "csv" | "tsv" | "csv.zip" | "tsv.zip" | "json"
        )
    {
        return crate::sheets::export(&r.info.path, out, &r.format, cancel);
    }
    let office_layout = e.soffice.is_some()
        && (office_text(&r.info.extension)
            || spreadsheet(&r.info.extension)
            || presentation(&r.info.extension))
        && matches!(
            r.format.as_str(),
            "pdf"
                | "doc"
                | "docx"
                | "odt"
                | "rtf"
                | "txt"
                | "xls"
                | "xlsx"
                | "ods"
                | "ppt"
                | "pptx"
                | "odp"
        );
    if let (Some(pandoc), Some(reader), false) =
        (&e.pandoc, doc_reader(&r.info.extension), office_layout)
    {
        if r.format != "pdf" || e.typst.is_some() {
            let staging = tempfile::tempdir()?;
            let bundle = r.format.ends_with(".zip");
            let extension = r.format.trim_end_matches(".zip");
            let rendered = staging.path().join(format!("document.{extension}"));
            let output = if bundle { rendered.as_path() } else { out };
            let mut cmd = Command::new(pandoc);
            cmd.current_dir(staging.path());
            cmd.arg(&r.info.path)
                .args(["--from", reader, "--standalone", "--output"])
                .arg(output);
            if bundle || extension == "html" {
                cmd.args(["--extract-media", "media"]);
            }
            if extension == "html" {
                cmd.arg("--embed-resources");
            }
            if r.format == "pdf" {
                cmd.arg("--pdf-engine").arg(e.typst.as_ref().unwrap());
            } else {
                cmd.args([
                    "--to",
                    match extension {
                        "txt" => "plain",
                        "md" => "gfm",
                        "tex" => "latex",
                        v => v,
                    },
                ]);
            }
            // Never run Lua filters or external filters from input metadata.
            if let Some(parent) = r.info.path.parent() {
                cmd.arg("--resource-path").arg(parent);
            }
            run_process(&mut cmd, cancel, cb, None, "Converting document")?;
            if bundle {
                zip_directory(staging.path(), out, cancel)?;
            }
            return Ok(());
        }
    }
    office_document(&r.info.path, &r.format, out, e, cancel, cb)
}

fn office_document(
    input: &Path,
    format: &str,
    out: &Path,
    e: &Engines,
    cancel: &Arc<AtomicBool>,
    cb: Notify<'_>,
) -> Result<()> {
    let office = e
        .soffice
        .as_ref()
        .ok_or_else(|| anyhow!("This conversion needs LibreOffice"))?;
    let directory = tempfile::tempdir()?;
    let profile = tempfile::tempdir()?;
    let uri = format!(
        "file:///{}",
        profile
            .path()
            .to_string_lossy()
            .replace('\\', "/")
            .replace(' ', "%20")
    );
    let mut cmd = Command::new(office);
    cmd.arg(format!("-env:UserInstallation={uri}"))
        .args(["--headless", "--convert-to", format, "--outdir"])
        .arg(directory.path())
        .arg(input);
    run_process(&mut cmd, cancel, cb, None, "Rendering Office document")?;
    let expected = directory
        .path()
        .join(input.file_stem().unwrap())
        .with_extension(format);
    fs::copy(&expected, out).context("LibreOffice did not produce the requested format")?;
    Ok(())
}

fn pdf(
    r: &Request,
    out: &Path,
    e: &Engines,
    cancel: &Arc<AtomicBool>,
    cb: Notify<'_>,
) -> Result<()> {
    if e.pdf_word.is_some()
        && matches!(
            r.format.as_str(),
            "png" | "jpg" | "png.zip" | "jpg.zip" | "svg.zip" | "xlsx" | "txt"
        )
    {
        return auxiliary(
            &r.info.path,
            out,
            e,
            cancel,
            cb,
            if r.compress { image_quality(r) } else { 100 },
        );
    }
    if matches!(r.format.as_str(), "docx" | "pptx" | "ppt" | "svg") {
        let executable = e.pdf_word.as_ref().ok_or_else(|| {
            anyhow!("The PDF conversion engine is missing. Restore the tools/pdf-word folder.")
        })?;
        let staging = tempfile::tempdir()?;
        let presentation = staging.path().join("slides.pptx");
        let target = if r.format == "ppt" {
            presentation.as_path()
        } else {
            out
        };
        let mut command = Command::new(executable);
        command.arg(&r.info.path).arg(target);
        run_process(
            &mut command,
            cancel,
            cb,
            None,
            if r.format == "docx" {
                "Reconstructing Word document"
            } else {
                "Reconstructing PDF pages"
            },
        )?;
        if r.format == "ppt" {
            office_document(&presentation, "ppt", out, e, cancel, cb)?;
        }
        return Ok(());
    }
    if matches!(r.format.as_str(), "png" | "jpg") {
        let temp = tempfile::tempdir()?;
        let base = temp.path().join("page");
        let mut cmd = Command::new(
            e.pdftoppm
                .as_ref()
                .ok_or_else(|| anyhow!("PDF rendering is unavailable"))?,
        );
        cmd.args([
            "-f",
            "1",
            "-singlefile",
            "-r",
            "150",
            if r.format == "png" { "-png" } else { "-jpeg" },
        ])
        .arg(&r.info.path)
        .arg(&base);
        if r.format == "jpg" && r.compress {
            cmd.arg("-jpegopt")
                .arg(format!("quality={}", image_quality(r)));
        }
        run_process(&mut cmd, cancel, cb, None, "Rendering first page")?;
        fs::copy(base.with_extension(&r.format), out)?;
        return Ok(());
    }
    notify(cb, Some(0.15), "Reading PDF");
    let mut doc = lopdf::Document::load(&r.info.path)?;
    check_cancel(cancel)?;
    if r.format == "txt" {
        let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
        let mut file = File::create(out)?;
        for (index, page) in pages.iter().enumerate() {
            check_cancel(cancel)?;
            file.write_all(doc.extract_text(&[*page])?.as_bytes())?;
            file.write_all(b"\n")?;
            notify(
                cb,
                Some(index as f32 / pages.len().max(1) as f32),
                "Extracting text",
            );
        }
        if file.metadata()?.len() <= pages.len() as u64 {
            bail!("No selectable text was found. This PDF may need OCR.");
        }
    } else {
        if r.compress {
            compress_pdf_images(&mut doc, image_quality(r), cancel)?;
            doc.prune_objects();
            doc.compress();
        }
        notify(cb, Some(0.8), "Writing PDF");
        doc.save(out)?;
    }
    Ok(())
}

fn compress_pdf_images(
    doc: &mut lopdf::Document,
    quality: u8,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    for object in doc.objects.values_mut() {
        check_cancel(cancel)?;
        let Ok(stream) = object.as_stream_mut() else {
            continue;
        };
        let name = |key: &[u8]| stream.dict.get(key).ok().and_then(|v| v.as_name().ok());
        if name(b"Subtype") != Some(b"Image")
            || !matches!(name(b"Filter"), Some(b"DCTDecode" | b"FlateDecode"))
            || !matches!(name(b"ColorSpace"), Some(b"DeviceRGB" | b"DeviceGray"))
        {
            continue;
        }
        if stream
            .dict
            .get(b"BitsPerComponent")
            .and_then(|v| v.as_i64())
            .ok()
            != Some(8)
            || stream.content.len() > 32 * 1024 * 1024
        {
            continue;
        }
        let gray = name(b"ColorSpace") == Some(b"DeviceGray");
        let deflated = name(b"Filter") == Some(b"FlateDecode");
        let decoded = if deflated {
            if stream.dict.has(b"DecodeParms")
                || stream.dict.has(b"Mask")
                || stream.dict.has(b"SMask")
            {
                continue;
            }
            let dimension = |key: &[u8]| {
                stream
                    .dict
                    .get(key)
                    .ok()
                    .and_then(|v| v.as_i64().ok())
                    .and_then(|v| u32::try_from(v).ok())
            };
            let (Some(w), Some(h)) = (dimension(b"Width"), dimension(b"Height")) else {
                continue;
            };
            let expected = w as u64 * h as u64 * if gray { 1 } else { 3 };
            if expected > 32 * 1024 * 1024 {
                continue;
            }
            let Ok(bytes) = stream.decompressed_content() else {
                continue;
            };
            if bytes.len() as u64 != expected {
                continue;
            }
            if gray {
                image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, bytes).unwrap())
            } else {
                image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, bytes).unwrap())
            }
        } else {
            let mut reader = image::ImageReader::new(std::io::Cursor::new(&stream.content))
                .with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(128 * 1024 * 1024);
            limits.max_image_width = Some(16000);
            limits.max_image_height = Some(16000);
            reader.limits(limits);
            let Ok(decoded) = reader.decode() else {
                continue;
            };
            decoded
        };
        let mut encoded = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, quality);
        if gray {
            encoder.encode_image(&decoded.to_luma8())?;
        } else {
            encoder.encode_image(&decoded.to_rgb8())?;
        }
        if encoded.len() < stream.content.len() {
            stream.set_content(encoded);
            stream
                .dict
                .set("Filter", lopdf::Object::Name(b"DCTDecode".to_vec()));
            stream.dict.remove(b"DecodeParms");
        }
    }
    Ok(())
}

fn read_structured(path: &Path, extension: &str) -> Result<serde_json::Value> {
    let text = fs::read_to_string(path).context("This file is not UTF-8 text")?;
    Ok(match extension {
        "json" => serde_json::from_str(&text)?,
        "yaml" | "yml" => serde_yaml::from_str(&text)?,
        "toml" => serde_json::to_value(toml::from_str::<toml::Value>(&text)?)?,
        _ => bail!("Unsupported structured data"),
    })
}
fn toml_compatible(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Number(n) => n.is_i64() || n.is_f64(),
        serde_json::Value::Array(values) => values.iter().all(toml_compatible),
        serde_json::Value::Object(values) => values.values().all(toml_compatible),
        _ => true,
    }
}
fn data(r: &Request, out: &Path, cancel: Option<&AtomicBool>) -> Result<()> {
    if r.info.bytes > 64 * 1024 * 1024 {
        bail!("Structured-data conversion is limited to 64 MB to keep memory bounded.");
    }
    let value: serde_json::Value = match r.info.extension.as_str() {
        "csv" | "tsv" => {
            let text = fs::read_to_string(&r.info.path).context("This file is not UTF-8 text")?;
            let mut csv = csv::ReaderBuilder::new()
                .delimiter(if r.info.extension == "tsv" {
                    b'\t'
                } else {
                    b','
                })
                .from_reader(text.as_bytes());
            let headers = csv.headers()?.clone();
            let unique: std::collections::HashSet<_> = headers.iter().collect();
            if unique.len() != headers.len() {
                bail!("Duplicate column names cannot be represented without losing data.");
            }
            let mut rows = Vec::new();
            for row in csv.records() {
                let row = row?;
                let object = headers
                    .iter()
                    .zip(row.iter())
                    .map(|(k, v)| (k.into(), serde_json::Value::String(v.into())))
                    .collect::<serde_json::Map<_, _>>();
                rows.push(serde_json::Value::Object(object));
            }
            serde_json::Value::Array(rows)
        }
        _ => read_structured(&r.info.path, &r.info.extension)?,
    };
    if matches!(r.format.as_str(), "xlsx" | "ods" | "pdf") {
        let never = AtomicBool::new(false);
        return crate::sheets::records_to_excel(&value, out, cancel.unwrap_or(&never));
    }
    let output = match r.format.as_str() {
        "json" => {
            if r.compress {
                serde_json::to_string(&value)?
            } else {
                serde_json::to_string_pretty(&value)?
            }
        }
        "yaml" => serde_yaml::to_string(&value)?,
        "toml" => toml::to_string_pretty(&value).context(
            "This data cannot be represented in TOML (for example, null values or a root array)",
        )?,
        "csv" | "tsv" => {
            let rows = value
                .as_array()
                .ok_or_else(|| anyhow!("CSV/TSV output needs a JSON array of flat objects"))?;
            let mut headers = std::collections::BTreeSet::new();
            for row in rows {
                let object = row
                    .as_object()
                    .ok_or_else(|| anyhow!("Each row must be a flat object"))?;
                for (k, v) in object {
                    if v.is_array() || v.is_object() {
                        bail!("Nested records cannot be written as a flat table without losing structure.");
                    }
                    headers.insert(k.clone());
                }
            }
            let headers: Vec<_> = headers.into_iter().collect();
            let mut writer = csv::WriterBuilder::new()
                .delimiter(if r.format == "tsv" { b'\t' } else { b',' })
                .from_writer(Vec::new());
            writer.write_record(&headers)?;
            for row in rows {
                writer.write_record(headers.iter().map(|h| match &row[h] {
                    serde_json::Value::Null => String::new(),
                    serde_json::Value::String(s) => s.clone(),
                    v => v.to_string(),
                }))?;
            }
            String::from_utf8(writer.into_inner()?)?
        }
        _ => bail!("Unsupported structured output"),
    };
    fs::write(out, output)?;
    Ok(())
}

fn zip_file(
    input: &Path,
    out: &Path,
    compression: u8,
    cancel: &AtomicBool,
    cb: Notify<'_>,
) -> Result<()> {
    let mut archive = zip::ZipWriter::new(File::create(out)?);
    archive.start_file(
        input.file_name().unwrap_or_default().to_string_lossy(),
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(
                (1 + compression.clamp(1, 100) as i64 * 8 / 100) as i64,
            )),
    )?;
    let mut source = File::open(input)?;
    let size = source.metadata()?.len().max(1);
    let mut buffer = [0u8; 128 * 1024];
    let mut read = 0u64;
    loop {
        check_cancel(cancel)?;
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        archive.write_all(&buffer[..n])?;
        read += n as u64;
        notify(cb, Some(read as f32 / size as f32), "Compressing into ZIP");
    }
    archive.finish()?;
    Ok(())
}

fn run_capture(cmd: &mut Command, timeout: Duration, limit: usize) -> Result<Vec<u8>> {
    crate::platform::quiet(cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn()?;
    #[cfg(windows)]
    let _group = crate::platform::ProcessGroup::attach(&child);
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let bytes = reader
                .join()
                .map_err(|_| anyhow!("Preview reader stopped"))??;
            if !status.success() {
                bail!("The file could not be decoded");
            }
            if bytes.len() > limit {
                bail!("Preview exceeds the size limit");
            }
            return Ok(bytes);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            bail!("File inspection timed out");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn run_process(
    cmd: &mut Command,
    cancel: &Arc<AtomicBool>,
    cb: Notify<'_>,
    duration: Option<f64>,
    label: &str,
) -> Result<()> {
    crate::platform::quiet(cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .context("Could not start the conversion engine")?;
    #[cfg(windows)]
    let group = crate::platform::ProcessGroup::attach(&child);
    let (send, receive) = std::sync::mpsc::sync_channel(64);
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    let stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut tail = VecDeque::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            tail.push_back(line.chars().take(2000).collect::<String>());
            if tail.len() > 12 {
                tail.pop_front();
            }
        }
        tail.into_iter().collect::<Vec<_>>().join("\n")
    });
    notify(cb, None, label);
    let status = loop {
        while let Ok(line) = receive.try_recv() {
            if let Some(fraction) = line
                .strip_prefix("dropform_progress=")
                .and_then(|v| v.parse::<f32>().ok())
            {
                notify(cb, Some(fraction.clamp(0.0, 0.99)), label);
            }
            if let Some(micros) = line
                .strip_prefix("out_time_us=")
                .and_then(|v| v.parse::<f64>().ok())
            {
                if let Some(total) = duration {
                    notify(
                        cb,
                        Some((micros / 1_000_000.0 / total).clamp(0.0, 0.98) as f32),
                        label,
                    );
                }
            }
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            #[cfg(windows)]
            drop(group);
            let _ = child.wait();
            drop(receive);
            let _ = reader.join();
            let _ = errors.join();
            bail!("Conversion canceled");
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    drop(receive);
    let _ = reader.join();
    let tail = errors.join().unwrap_or_default();
    if !status.success() {
        bail!(
            "Conversion engine failed: {}",
            tail.lines()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    Ok(())
}

pub fn cli(args: &[String], engines: &Engines) -> Result<()> {
    if args.len() < 3 {
        bail!("Usage: Dropflip --convert INPUT FORMAT OUTPUT_FOLDER [--compress]");
    }
    let info = inspect(Path::new(&args[0]), engines)?;
    let request = Request {
        info,
        format: args[1].to_lowercase(),
        destination: PathBuf::from(&args[2]),
        compress: args.iter().any(|s| s == "--compress"),
        compression: args
            .iter()
            .position(|s| s == "--compression")
            .map(|i| {
                args.get(i + 1)
                    .ok_or_else(|| anyhow!("Expected compression 1–100"))
                    .and_then(|s| {
                        s.parse::<u8>()
                            .map_err(|_| anyhow!("Expected compression 1–100"))
                    })
            })
            .transpose()?
            .unwrap_or(40)
            .clamp(1, 100),
    };
    let result = convert(
        &request,
        engines,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_stops_a_running_media_process() {
        let Some(ffmpeg) = Engines::discover().ffmpeg else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            signal.store(true, Ordering::Relaxed);
        });
        let mut command = Command::new(ffmpeg);
        command.args([
            "-v",
            "error",
            "-nostdin",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "color=size=16x16:rate=1",
            "-t",
            "60",
            "-f",
            "null",
            "-",
        ]);
        let start = Instant::now();
        assert!(
            run_process(&mut command, &cancel, &mut |_| {}, None, "test")
                .unwrap_err()
                .to_string()
                .contains("canceled")
        );
        worker.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(5));
    }
    #[test]
    fn data_conversion_preserves_records_and_does_not_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("records.csv");
        fs::write(&input, "name,value\nalpha,42\n").unwrap();
        let engines = Engines::default();
        let request = Request {
            info: inspect(&input, &engines).unwrap(),
            format: "json".into(),
            destination: temp.path().to_path_buf(),
            compress: false,
            compression: 40,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let first = convert(&request, &engines, &cancel, &mut |_| {}).unwrap();
        let second = convert(&request, &engines, &cancel, &mut |_| {}).unwrap();
        assert_ne!(first.path, second.path);
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&first.path).unwrap()).unwrap();
        assert_eq!(value[0]["value"], "42");
        assert!(input.exists());
    }
    #[test]
    fn cancellation_never_publishes_a_file() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("x.txt");
        fs::write(&input, "hello").unwrap();
        let request = Request {
            info: inspect(&input, &Engines::default()).unwrap(),
            format: "zip".into(),
            destination: temp.path().join("output"),
            compress: true,
            compression: 40,
        };
        assert!(convert(
            &request,
            &Engines::default(),
            &Arc::new(AtomicBool::new(true)),
            &mut |_| {}
        )
        .is_err());
        assert!(!request.destination.exists());
    }
    #[test]
    fn nested_data_refuses_flattening() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("x.json");
        fs::write(&input, r#"[{"a":{"b":1}}]"#).unwrap();
        let request = Request {
            info: inspect(&input, &Engines::default()).unwrap(),
            format: "csv".into(),
            destination: temp.path().join("output"),
            compress: false,
            compression: 40,
        };
        assert!(convert(
            &request,
            &Engines::default(),
            &Arc::new(AtomicBool::new(false)),
            &mut |_| {}
        )
        .is_err());
        assert!(
            !request.destination.exists()
                || fs::read_dir(&request.destination).unwrap().count() == 0
        );
    }
}
