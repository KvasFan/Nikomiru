# SPDX-License-Identifier: GPL-3.0-only
# Nikomiru — English interface strings (SET-004).

app-title = Nikomiru
open-file = Open file…
file-filter-video = Video files
file-filter-all = All files
drop-hint = Drop a video here, or press Ctrl+O to open one

play = Play
pause = Pause
fullscreen = Fullscreen
exit-fullscreen = Exit fullscreen
volume = Volume
mute = Mute
unmute = Unmute
speed = Speed
audio-track = Audio track
no-audio = No audio
track-unknown-language = Unknown language
chapter-previous = Previous chapter
chapter-next = Next chapter

toast-volume = Volume { $value }%
toast-speed = Speed { $value }×
toast-muted = Muted
toast-unmuted = Sound on
toast-paused = Paused
toast-playing = Playing

error-file = Cannot play “{ $file }”: { $reason }.

# Settings (nikomiru-core ConfigError, PathsError)
error-config = The settings file { $path } is not valid at line { $line } ({ $reason }). The defaults are in use until you fix it.
error-config-read = The settings file { $path } cannot be read ({ $reason }). The defaults are in use.
error-config-write = The settings cannot be saved to { $path } ({ $reason }). Check that the folder is writable.
error-config-serialize = The settings cannot be saved ({ $reason }).
error-paths = HOME is not set: Nikomiru cannot find its folders.

# Library (nikomiru-library LibraryError)
error-library = The library cannot be used ({ $reason }): positions and history are not saved.
error-library-record = A library record is damaged ({ $reason }).
error-library-io = The library file { $path } cannot be used ({ $reason }).
error-library-read-only = The library was written by a newer version of Nikomiru: it is open read-only.
error-library-schema = The library file is damaged: it has no valid version. Nikomiru leaves it as it is.

# Player (nikomiru-player PlayerError, PlaybackError)
error-player = The player cannot do this ({ $reason }).
error-player-thread = The player cannot start ({ $reason }).
playback-loading-failed = the file is missing, unreadable or damaged
playback-audio-output = no audio output is available
playback-video-output = the video output cannot start
playback-nothing-to-play = the file has no audio or video
playback-unknown-format = the file format is not recognized
playback-unsupported = the file is not supported
playback-other = mpv error { $code }

# FFmpeg (nikomiru-media ToolsError)
error-ffmpeg-missing = FFmpeg was not found. Install it, or set its path in Settings → Advanced: until then, subtitle extraction and card media are off.
error-ffprobe-missing = FFmpeg was found at { $path }, but ffprobe is not next to it.
error-ffmpeg-run = { $path } cannot be run ({ $reason }).
error-ffmpeg-timeout = { $path } did not answer within { $seconds } seconds.
error-ffmpeg-old = FFmpeg { $version } is too old: version { $minimum } or newer is needed.

# External processes (nikomiru-media JobError)
error-job-spawn = { $program } cannot be started ({ $reason }).
error-job-timeout = { $program } did not finish within { $seconds } seconds and was stopped.
error-job-cancelled = { $program } was cancelled.
error-job-failed = { $program } failed: { $reason }

# Single instance (nikomiru-app InstanceError)
error-instance-io = { $path } cannot be used ({ $reason }): more Nikomiru windows may open.
error-instance-no-answer = Nikomiru is already running but does not answer. Close it and try again.

# Shown by the desktop while the screensaver is inhibited (PLY-013).
inhibit-reason = Playing a video
