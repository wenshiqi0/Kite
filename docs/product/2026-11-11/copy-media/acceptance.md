# Native selection and attachment playback

The shared Item renderer no longer adds a permanent copy button or its 40 dp
footer. The unchanged native Markdown TextView remains text-selectable for the
platform's normal selection/copy actions. Sender backgrounds, padding and
alignment retain their existing behavior.

Validation:

- 85 Android Release JVM tests, Release lint, app compilation and native test
  compilation passed.
- Two Android 16 native tests passed: the shared content/distinct bubbles image
  preview test, and native audio/video/text attachment playback. Screenshot
  inspection confirms both authors' copy icons and footer space are gone.
- The video fixture now contains both H.264 video and AAC audio. The exact
  four-second sample supplied to the user was used by the native playback test;
  the emulator initialized the AAC and AVC decoders. Playback remains manual.
- The installed host projected the owned JPEG, WAV and MP4 files in order as
  image/jpeg, audio/wav and video/mp4 attachments. ffprobe verified the three-
  second PCM audio and four-second H.264/AAC video.

The candidate includes the existing native-history/navigation fixes and the
verified CI package-resolution fix. Only the message copy control and owned
media test fixture change beyond that baseline. Physical-handset audibility is
left to the supplied samples. Publication verification is recorded below.
