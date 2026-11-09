# AK-BUG-005: KANJIDIC and Kanjium fail with HTTP 504 in the setup assistant

**Symptom.** At the first setup, KANJIDIC and Kanjium's pitch accents showed "HTTP 504" and were not installed. Reported by the maintainer on 2.18.0.

**Steps to reproduce.** Open the setup assistant with the default dictionaries ticked and press Install while GitHub's release downloads answer a gateway timeout, which they do now and then. The same addresses answered 200 a few minutes later.

**Cause.** A dictionary was requested once: any failure, even one that passes by itself, failed it. The audio pack's requests were already tried three times (INS-004); the dictionaries' were not.

**Fix.** A dictionary's download is tried again from the start when its failure may pass by itself (no answer, a timeout, HTTP 429 or 5xx), three times in all, 2 s and then 4 s apart; other failures, such as a 404 or a wrong SHA-256, fail at once. Pressing Install again still tries the failed ones. Test `ak_bug_005_a_passing_failure_is_tried_again` answers 504 and 502 before the file, three 504s, and a 404.
