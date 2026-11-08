# AK-BUG-004: the setup assistant does not open from the menu

**Symptom.** "Set up dictionaries, audio and cards…" in Settings → General, or in the library's first steps, seems to do nothing. Reported by the maintainer on 2.17.0.

**Steps to reproduce.** Open Settings → General and click the setup assistant's button, or click it in the library's first steps.

**Cause.** The assistant was drawn in egui's `Middle` layer, the library and the settings in the `Foreground` layer as full pages of their own. The assistant opened, but under the page it was opened from, which hid it and took its clicks. It showed only at the first start, over the player.

**Fix.** The assistant is drawn in the `Foreground` layer: an area that was not shown in the frame before is raised to the top of its layer, so it comes above the page it is opened from. Test `ak_bug_004_the_setup_assistant_opens_above_its_page` opens it over a full page and clicks its Close button.
