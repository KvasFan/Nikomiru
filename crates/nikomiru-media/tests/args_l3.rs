// SPDX-License-Identifier: GPL-3.0-only

//! L3: the ffmpeg argument vector of a subtitle extraction (T-EXT-002).

use std::path::{Path, PathBuf};

use nikomiru_media::extract::{Extractor, Stream};

// T-EXT-002
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
    // a name that looks like an option and like a protocol
    let video = Path::new("/videos/-weird: name.mkv");
    let args = Extractor::args(
        video,
        &[
            (&srt, PathBuf::from("/cache/a-3.srt.part")),
            (&ass, PathBuf::from("/cache/a-4.ass.part")),
        ],
    );
    let shown: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // every path goes through `file:`, nothing through a shell
    insta::assert_snapshot!(shown.join("\n"));
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
