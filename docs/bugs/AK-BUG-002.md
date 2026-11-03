# AK-BUG-002: the answer of a review card hides itself again

**Symptom.** On the Review tab, "Show the answer" shows the back of the card, and a moment later the card is back on its front.

**Steps to reproduce.** Open the Library page on the Review tab, or open the page and click Review at once, and press "Show the answer" before the tab has finished reading. The GUI test `a_card_made_for_the_review_comes_back_in_its_tab` met it twice in a row in the full suite of 2.10.1, waiting for "revealed".

**Cause.** Opening the Library page reads the review (for the number due), and so does the Review tab: two reads run one after the other. The page was busy until the first one answered, so it took the answer then; the second read, answering after, hid the answer of every card, even of the card still shown.

**Fix.** The page counts the jobs asked and stays busy until the last one is answered. A read that shows the same card again keeps its answer in sight; a different card, an error, or a job that changes the cards (an answer, Undo, Delete, Start over) shows the front. Test `ak_bug_002_a_second_read_keeps_the_answer_shown`.
