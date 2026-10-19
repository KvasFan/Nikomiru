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
