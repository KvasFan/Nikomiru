# AK-BUG-001: a toast wraps at the width of the one before

**Symptom.** A toast wraps early: "Card added to Lapis." takes two lines in English, and five in Japanese, with room to spare in the window.

**Steps to reproduce.** Show a short toast, such as the volume, then a longer one, such as the card added.

**Cause.** An egui `Area` keeps the size of its last frame. Every toast uses the same area, so a new text wraps at the width of the toast before it. Japanese makes it plain, since it can break at any character.

**Fix.** The toast measures its text, up to 80 % of the window, and sets that width itself. A new area for each toast would also work, but would leave an entry in egui's memory for each one (T-REL-003).
