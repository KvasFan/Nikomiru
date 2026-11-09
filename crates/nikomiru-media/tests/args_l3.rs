// SPDX-License-Identifier: GPL-3.0-only

//! L3: the ffmpeg argument vector of a subtitle extraction (T-EXT-002).

use std::path::{Path, PathBuf};

use nikomiru_media::extract::{Extractor, Stream};

// T-EXT-002, T-SUB-028, T-SUB-029
#[test]
fn extraction_arguments() {
    let srt = Stream {
        ff_index: 3,
        codec: "subrip".into(),
    };
    let ass = Stream {
        ff_index: 4,
        codec: "ass".into(),
    };
    // SUB-028: pictures copied as they are
    let pgs = Stream {
        ff_index: 5,
        codec: "hdmv_pgs_subtitle".into(),
    };
    // SUB-029: and VobSub, as a program stream that starts on time
    let vob = Stream {
        ff_index: 6,
        codec: "dvd_subtitle".into(),
    };
    // a name that looks like an option and like a protocol
    let video = Path::new("/videos/-weird: name.mkv");
    let args = Extractor::args(
        video,
        &[
            (&srt, PathBuf::from("/cache/a-3.srt.part")),
            (&ass, PathBuf::from("/cache/a-4.ass.part")),
            (&pgs, PathBuf::from("/cache/a-5.sup.part")),
            (&vob, PathBuf::from("/cache/a-6.sub.part")),
        ],
    );
    let shown: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // every path goes through `file:`, nothing through a shell
    insta::assert_snapshot!(shown.join("\n"));
}

// T-SUB-029: a VobSub track's palette, from ffprobe's hex dump of its
// private data
#[test]
fn the_private_data_is_read_from_its_dump() {
    use nikomiru_media::extract::{parse_private_data, private_data_args};
    let args: Vec<String> = private_data_args(Path::new("/videos/-a.mkv"), 3)
        .into_iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args[3], "3");
    assert_eq!(args.last().unwrap(), "file:/videos/-a.mkv");
    let json = r#"{"streams": [{"extradata": "\n00000000: 7369 7a65 3a20 3139 3230 7831 3038 300a  size: 1920x1080.\n00000010: 7061 6c65 7474 653a 2030 3030 3030 302c  palette: 000000,\n00000020: 2066 6666 6666 660a                        ffffff.\n"}]}"#;
    let data = parse_private_data(json).unwrap();
    assert_eq!(
        String::from_utf8(data).unwrap(),
        "size: 1920x1080\npalette: 000000, ffffff\n"
    );
    assert_eq!(parse_private_data(r#"{"streams": [{}]}"#), None);
    assert_eq!(parse_private_data("not json"), None);
}

// T-ANK-040: the picture at the start of a sentence, taken from the video
#[test]
fn a_frame_is_taken_at_its_time_whole() {
    let args: Vec<String> = nikomiru_media::clip::frame_args(
        Path::new("/videos/-weird: name.mkv"),
        61_250,
        Path::new("/cache/f.png"),
    )
    .into_iter()
    .map(|a| a.to_string_lossy().into_owned())
    .collect();
    let at = args.iter().position(|a| a == "-ss").unwrap();
    assert_eq!(args[at + 1], "61.250");
    let input = args.iter().position(|a| a == "-i").unwrap();
    assert!(at < input, "the seek before the input is exact and fast");
    assert_eq!(args[input + 1], "file:/videos/-weird: name.mkv");
    assert_eq!(args.last().unwrap(), "file:/cache/f.png");
    assert!(
        !args.iter().any(|a| a.contains("scale")),
        "the frame keeps its size"
    );
}
