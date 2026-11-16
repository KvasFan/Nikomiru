# SPDX-License-Identifier: GPL-3.0-only
# Nikomiru — Japanese interface strings (SET-004).

app-title = Nikomiru
open-file = ファイルを開く…
file-filter-video = 動画ファイル
file-filter-all = すべてのファイル
drop-hint = ここに動画をドロップするか、Ctrl+O で開きます

play = 再生
pause = 一時停止
fullscreen = 全画面
exit-fullscreen = 全画面を終了
volume = 音量
mute = ミュート
unmute = ミュート解除
speed = 速度
speed-up = 速く
speed-down = 遅く
position = 再生位置
time-switch = クリックで残り時間と経過時間を切り替え
video-area = 動画
audio-track = 音声トラック
track-unknown-language = 言語不明

toast-volume = 音量 { $value }%
toast-speed = 速度 { $value }×
toast-screenshot = スクリーンショットを保存しました: { $name }
toast-screenshot-failed = スクリーンショットを保存できません（{ $reason }）。
card-button = Anki カード
word-status-unknown = 新しい
word-status-learning = 学習中
word-status-known = 既知
word-status-ignored = 無視
card-overwrite = 上書き
card-open-in-anki = Anki で開く
card-add-anyway = それでも追加
toast-card-added = { $deck } にカードを追加しました。
toast-card-overwritten = { $deck } のカードを上書きしました。
toast-card-kept = カードを後で送るために保存しました（{ $count } 枚）。設定 → Anki から送れます。
toast-later-sent = { $sent } 枚のカードを Anki に送りました。
toast-later-some-failed = Anki に送信：{ $sent } 枚。未送信：{ $failed } 枚（{ $reason }）。
link-missing = このカードの動画が見つかりません:
link-locate = ファイルの場所を指定…
anki-open = Anki 設定
anki-save = 保存
anki-connection = 接続
anki-url = AnkiConnect のアドレス
anki-key = API キー
anki-show-key = 表示
anki-test = 接続をテスト
anki-create-lapis = Lapis デッキを作成
anki-note = ノート
anki-deck = デッキ
anki-note-type = ノートタイプ
anki-fields = フィールド
anki-fields-hint = 接続をテストすると、ノートタイプのフィールドが表示されます。
anki-insert = プレースホルダーを挿入
anki-lapis-defaults = Lapis の既定値
anki-duplicates = 重複
anki-scope-deck = デッキ内
anki-scope-collection = コレクション全体
anki-tags = タグ（スペース区切り）
anki-media = メディア
anki-pad-head = 字幕の前の音声（ミリ秒）
anki-pad-tail = 字幕の後の音声（ミリ秒）
anki-audio-format = 音声形式
anki-audio-quality = 音質
anki-audio-q0 = モノラル 64 kbit/s
anki-audio-q1 = ステレオ 128 kbit/s
anki-audio-q2 = ステレオ 192 kbit/s
anki-image-format = 画像形式
anki-image-quality = 画像サイズ
anki-image-q0 = 720 px
anki-image-q1 = 1000 px
anki-image-q2 = 2000 px
anki-image-q3 = 元のサイズ
anki-image-webp-animated = 文のアニメーション WebP
anki-image-avif-animated = 文のアニメーション AVIF（小さいが遅い）
anki-image-webm = 文の WebM 動画（音声付き）
anki-image-a0 = 480 px、10 フレーム/秒
anki-image-a1 = 640 px、12 フレーム/秒
anki-image-a2 = 854 px、15 フレーム/秒
anki-image-a3 = 1280 px、15 フレーム/秒
anki-connected = 接続しました: AnkiConnect { $version }、デッキ { $decks } 個、ノートタイプ { $models } 個。
anki-lapis-created = Lapis のノートタイプとデッキを Anki に作成しました。
anki-lapis-exists = Lapis のノートタイプとデッキは Anki にすでにあります。
settings-saved = 設定を保存しました。
settings-title = 設定
settings-close = 閉じる
settings-player = プレーヤー
settings-services = 字幕サービス
settings-dictionaries = 辞書
settings-audio = 単語の音声
settings-anki = Anki
settings-general = 一般
settings-subtitles = 字幕
settings-lookup = 検索
settings-shortcuts = ショートカット
settings-advanced = 詳細
settings-about = このアプリについて
settings-save = 保存
settings-reset = リセット
settings-reset-hint = このタブの設定を既定値に戻します。保持するには「保存」を押してください。
settings-not-saved = config.toml を読み込めなかったため、書き込みませんでした。これらの設定は Nikomiru を閉じるまで有効です。ファイルを修正して、もう一度保存してください。
settings-interface = インターフェース
settings-language = 言語
settings-language-system = システムに合わせる
settings-theme = テーマ
settings-theme-system = デスクトップに合わせる
settings-theme-dark = ダーク
settings-theme-light = ライト
settings-keys = キーとパスワード
settings-keyring = システムのキーリングに保管する
settings-keyring-hint = 保存時に、API キー、パスワード、サーバーのトークンを config.toml ではなくデスクトップのキーリング（GNOME Keyring、KWallet）に保管します。キーリングに保管されたことを確認してから、ファイルから消去します。キーリングがロックされている場合は、Nikomiru の起動時にパスワードを求められることがあります。
settings-accent = アクセントカラー
settings-scale = インターフェースの拡大率
settings-playback = 再生
settings-seek = シークの間隔
settings-fit = 映像の表示
settings-fit-contain = 全体（黒帯あり）
settings-fit-cover = ウィンドウを埋める（端を切る）
settings-fit-stretch = ウィンドウに合わせて伸ばす
settings-auto-next = カウントダウンの後、シリーズの次の話を再生する
settings-gap-speed = 字幕の行間の再生速度
settings-gap-speed-off = オフ
settings-gap-speed-hint = 3秒以上の字幕の間を速く再生します。台詞とその前後は選択した速度で再生します。テキスト字幕が必要です。
settings-sub-size = サイズ
settings-sub-languages = 優先する言語
settings-sub-obscure = メイン字幕
settings-sub-secondary-obscure = サブ字幕
settings-sub-secondary-position = サブ字幕の位置
settings-position-top = 動画の上端
settings-position-bottom = メイン字幕の上
settings-sub-hint = サイズは高さ 1080 ピクセルの動画を基準にしたポイント数です。インターフェースの拡大率ではなく、動画の大きさに合わせて変わります。言語は ja や jpn のようなコードで、優先する順に書きます。
settings-sub-ass = ASS 字幕のスタイル（フォント、色、位置）を保持する
settings-sub-read-graphic = Blu-ray（PGS）と DVD（VobSub）の画像字幕を文字として読む
settings-sub-read-graphic-hint = マンガの読み取りモデルがインストールされていれば、自動で選ばれた画像字幕をこのコンピューターで読み取り、単語をクリックできるようにします。文字ができるまでは画像のまま表示し、読み取った文字は次回のために保存します。
settings-sub-furigana = 辞書の読みを漢字の上に表示する
settings-sub-mark-mined = カードにした単語に下線を引く
settings-sub-words-hint = 字幕は手持ちの辞書で単語に分割されます。単語の切り方を誤ると、読みも誤ることがあります。
settings-sub-word-colors = 単語を知っている度合いで色分けする
settings-sub-word-colors-hint = 字幕、本やウェブページで、まだ知らない単語には青、学習中の単語には黄色の下線を引きます。新しい単語がひとつだけの字幕行は一覧に +1 と表示します。辞書のポップアップで、単語ごとに「新しい」「学習中」「既知」「無視」を選べます。カードにした単語は学習中として扱います。
settings-sub-furigana-unknown-only = 知らない単語にだけ読みを表示する
settings-sub-known-lines = 新しい単語のない行
settings-sub-pause-new-only = 新しい単語のある行の終わりだけで一時停止
settings-sub-pitch-colors = 単語のアクセントを色で表示
settings-sub-pitch-colors-hint = アクセント辞書から各単語の上に線を引きます：青は平板、赤は頭高、橙は中高、緑は尾高、紫は起伏。
settings-known-words = Anki の既知語と単語リスト
settings-known-decks = カードを既知語とするデッキ
settings-known-field = 単語のフィールド
settings-known-hint = デッキはコンマで区切ります。間隔が 21 日以上のカードの単語は既知、学習したことのあるカードの単語は学習中になります。新規カードと保留中のカードは数えません。色分けが有効なら Nikomiru の起動のたびに読み込み、Anki を閉じているときのために保存します。ポップアップで選んだ状態が優先されます。
settings-known-read = 今すぐ Anki から読み込む
words-import = 単語リストを読み込む…
words-export = 単語を書き出す…
words-filter = 単語リスト
words-list-hint = リストは 1 行に 1 語（読みを問わず既知）か、「単語,読み,状態」の行です。状態は unknown、learning、known、ignored のいずれかです。書き出しは後者の形式です。
toast-known-no-decks = 先に「設定 → 字幕」で、カードを既知語とするデッキを選んでください
toast-known-failed = Anki から既知語を読み込めませんでした：{ $reason }
toast-known-read = Anki から：既知 { $known } 語、学習中 { $learning } 語
toast-words-imported = リストから { $count } 語を読み込みました
toast-words-import-failed = 単語リストを読み込めませんでした：{ $reason }
toast-words-exported = { $count } 語を { $path } に書き出しました
toast-words-export-failed = 単語を書き出せませんでした：{ $reason }
settings-sub-reveal = ぼかし・非表示の字幕をポインターの下で表示する
settings-sub-look = 見た目
settings-sub-font = フォント
settings-sub-font-default = インターフェースと同じ
settings-sub-furigana-size = ふりがなの大きさ
settings-sub-text-color = 文字の色
settings-sub-bold = 太字
settings-sub-outline = 縁取り
settings-sub-shadow = 影
settings-sub-box = 文字の背景
settings-sub-position = メイン字幕
settings-sub-edge-bottom = 下
settings-sub-edge-top = 上
settings-sub-margin = 端からの距離
settings-sub-look-hint = 距離と縁取りは高さ 1080 ピクセルの動画でのピクセル数です。透明な影や背景は描かれません。ASS のスタイルで描く字幕は、ファイルの色のままです。
settings-sub-list = 字幕リスト
settings-sub-list-text = 文字の大きさ
settings-sub-list-width = 幅
settings-lookup-results = ポップアップの検索結果
settings-popup-width = ポップアップの最大幅
settings-popup-height = ポップアップの最大の高さ
settings-lookup-hover = Shift なしで、ポインターを重ねて単語を検索する
settings-lookup-resume = ポインターがポップアップから離れたら再生を再開する
settings-ffmpeg = ffmpeg プログラム
settings-ffmpeg-hint = 空欄: PATH の ffmpeg
settings-next-start = ツールのパスの変更は、Nikomiru を再起動せずに次の処理から反映されます。
settings-folders = ファイルとフォルダー
settings-config-file = 設定ファイル
settings-logs = ログ
settings-copy = コピー
settings-mpv-hint = mpv のオプションは「プレーヤー」タブにあります。
about-tagline = クリックできる字幕、辞書、Anki カードで日本語を学ぶための動画プレーヤー。
about-license = ライセンス:
about-credits = クレジット
about-derived = Nikomiru は Fushi を Rust で書き直したもので、GPL-3.0 のコードから派生しています: Fushi、Yomitan（活用変換の表）、hoshidicts、Lapis ノートタイプ（テンプレートは変更なしで同梱）。ロゴとアイコンはこのプロジェクトのオリジナル作品で、同じライセンスで提供されます。
about-third-party = サードパーティーソフトウェア
about-third-party-file = ライセンス
about-third-party-missing = サードパーティーソフトウェアのライセンス（THIRD_PARTY_LICENSES.html）はパッケージに含まれますが、このビルドにはありません。cargo xtask about で作成できます。
shortcuts-hint = キーは物理的な位置で扱われます。キーボード配列が変わっても、ショートカットは同じキーのままです。入力欄に入力している間、単独のキーは働きません。
shortcuts-global = どこでも
shortcuts-player = プレーヤー
shortcuts-popup = 辞書ポップアップ
shortcuts-add = { $action } にキーを追加
shortcuts-remove = { $action } から { $key } を削除
shortcuts-default = { $action } のキーを既定に戻す
shortcuts-press = キーを押してください…（Esc で中止）
shortcuts-taken = { $key } はすでに { $actions } に使われています。
shortcuts-conflict = 次のキーが重複しています: { $keys }。保存する前に変更してください。
action-play-pause = 再生/一時停止
action-seek-back = 後ろへシーク
action-seek-forward = 前へシーク
action-volume-up = 音量を上げる
action-volume-down = 音量を下げる
action-mute = ミュート
action-speed-up = 速く
action-speed-down = 遅く
action-speed-reset = 標準の速度
action-hold-speed = 押している間 2 倍速
action-frame-prev = 前のフレーム
action-frame-next = 次のフレーム
action-screenshot = フレームを保存
action-screenshot-subtitles = 字幕付きでフレームを保存
action-fullscreen = 全画面
action-chapter-prev = 前のチャプター
action-chapter-next = 次のチャプター
action-back = 戻る
action-open-file = ファイルを開く
action-sub-prev = 前の字幕
action-sub-next = 次の字幕
action-replay-cue = 字幕をもう一度再生
action-replay-prev-cue = 前の字幕をもう一度再生
action-sub-list = 字幕リスト
action-sub-search = 字幕を検索
action-sub-blur = 字幕をぼかす
action-sub-hide = 字幕を隠す
action-sub-toggle = 字幕のオン/オフ
action-sub-auto-pause = 台詞ごとに一時停止
action-sub-delay-minus = 字幕を 100 ミリ秒早く
action-sub-delay-plus = 字幕を 100 ミリ秒遅く
action-sub-align-prev = 前の字幕をここに合わせる
action-sub-align-next = 次の字幕をここに合わせる
action-popup-next = 次の項目
action-popup-prev = 前の項目
action-create-card = カードを作成
mining-advanced-title = { $word } のカード
mining-sentence = 文
mining-no-sentence = 字幕の行がないため、カードに文は入りません。
mining-add-previous = 前の行を追加
mining-drop-previous = 最初の行を削除
mining-add-next = 次の行を追加
mining-drop-next = 最後の行を削除
mining-audio-section = 文の音声
mining-audio = 文の音声をカードに入れる
mining-start = 開始
mining-end = 終了
mining-preview = ▶ 聞く
mining-picture = 画像
mining-picture-click = クリックした場面のフレーム
mining-picture-start = 文の始まりのフレーム
mining-picture-none = 画像なし
mining-create = カードを作成
mining-cancel = キャンセル
action-create-card-advanced = カードを手動で作成
action-lookup-subtitle = 表示中の字幕を検索
action-lookup-prev = 前の単語から検索
action-lookup-next = 次の単語から検索
action-settings = 設定
anki-config-not-saved = config.toml を読み込めなかったため、書き込みませんでした。これらの設定は Nikomiru を閉じるまで有効です。ファイルを修正して、もう一度保存してください。
anki-deck-missing = Anki にデッキ { $name } がありません。別のデッキを選ぶか、作成してください。
anki-note-type-missing = Anki にノートタイプ { $name } がありません。別のものを選ぶか、Lapis を作成してください。
toast-muted = ミュート中
toast-unmuted = 音声オン

error-file = 「{ $file }」を再生できません: { $reason }。

# Settings (nikomiru-core ConfigError, PathsError)
error-config = 設定ファイル { $path } の { $line } 行目が正しくありません（{ $reason }）。修正するまで既定値を使用します。
error-config-read = 設定ファイル { $path } を読み込めません（{ $reason }）。既定値を使用します。
error-config-write = 設定を { $path } に保存できません（{ $reason }）。フォルダーに書き込めるか確認してください。
error-config-dir = 設定フォルダー { $path } に書き込めません（{ $reason }）。設定は保存されません。sudo で作成した場合は、所有者を自分に戻してください: sudo chown -R $USER: { $path }
error-config-serialize = 設定を保存できません（{ $reason }）。
error-config-values = { $path } の一部の設定は値の種類が正しくないため、既定値を使用します: { $keys }。設定を保存すると、既定値で上書きされます。
error-paths = HOME が設定されていないため、Nikomiru は自分のフォルダーを見つけられません。
error-keyring = システムのキーリングを読み込めません（{ $reason }）。読み込めるようになるまで、キーとパスワードは使用できません。何も削除されていません。
error-keyring-kept = システムのキーリングに書き込めません（{ $reason }）。キーとパスワードは当面 config.toml に残ります。

# Library (nikomiru-library LibraryError)
error-library = ライブラリを使用できません（{ $reason }）。再生位置と履歴は保存されません。
error-library-record = ライブラリのレコードが破損しています（{ $reason }）。
error-library-io = ライブラリファイル { $path } を使用できません（{ $reason }）。
error-library-read-only = ライブラリは新しいバージョンの Nikomiru で書き込まれています。読み取り専用で開きました。
error-library-schema = ライブラリファイルが破損しています。有効なバージョンがありません。Nikomiru はファイルをそのままにします。
error-library-unavailable = ライブラリを使用できません。ログを確認してください。

# Player (nikomiru-player PlayerError, PlaybackError)
error-player = プレーヤーはこの操作を実行できません（{ $reason }）。
error-player-thread = プレーヤーを起動できません（{ $reason }）。
error-player-path = プレーヤーはこのパスを使用できません: { $path }。
error-video-output = 動画を表示できません: { $reason }
playback-loading-failed = ファイルがないか、読み取れないか、破損しています
playback-audio-output = 使用できる音声出力がありません
playback-video-output = 映像出力を開始できません
playback-nothing-to-play = ファイルに音声も映像もありません
playback-unknown-format = ファイル形式を認識できません
playback-unsupported = このファイルには対応していません
playback-other = mpv エラー { $code }

# FFmpeg (nikomiru-media ToolsError)
error-ffmpeg-missing = FFmpeg が見つかりません。インストールするか、設定 → 詳細でパスを指定してください。それまで字幕の抽出とカードのメディア作成は無効です。
error-ffprobe-missing = FFmpeg は { $path } にありますが、同じ場所に ffprobe がありません。
error-ffmpeg-run = { $path } を実行できません（{ $reason }）。
error-ffmpeg-timeout = { $path } は { $seconds } 秒以内に応答しませんでした。
error-ffmpeg-old = FFmpeg { $version } は古すぎます。バージョン { $minimum } 以降が必要です。

# External processes (nikomiru-media JobError)
error-job-spawn = { $program } を起動できません（{ $reason }）。
error-job-timeout = { $program } は { $seconds } 秒以内に終わらなかったため、停止しました。
error-job-cancelled = { $program } はキャンセルされました。
error-job-failed = { $program } が失敗しました: { $reason }

# Dictionaries (nikomiru-dict ImportError, StoreError)
error-dict-io = { $path } を使用できません（{ $reason }）。
error-dict-zip = 有効な zip アーカイブではありません（{ $reason }）。
error-dict-foreign = { $file } を読み込めません（{ $reason }）。
error-dict-unsafe = 辞書に安全でないファイル（{ $entry }）が含まれているため、インポートしませんでした。
error-dict-no-index = Yomitan の辞書ではありません。index.json がありません。
error-dict-format = { $file } は Yomitan の形式ではありません（{ $reason }）。
error-dict-too-many = 辞書の項目数が 1000 万を超えています。
error-dict-glossary = { $expression } の定義が 10 MiB を超えています。
error-dict-empty = 辞書に Nikomiru で使える内容がありません。
error-dict-storage-full = ディスクがいっぱいです。空き容量を増やして、もう一度インポートしてください。
error-dict-cancelled = インポートをキャンセルしました。
error-dict-store = 辞書データベースを使用できません（{ $reason }）。
error-dict-same-revision = { $title } のリビジョン { $revision } はすでにインストールされています。
error-dict-unknown = この辞書はもう存在しません。

# Anki (nikomiru-anki AnkiError, ANK-004)
error-anki-bad-url = { $url } は AnkiConnect のアドレスではありません。http:// または https:// で始まります。
error-anki-refused = { $url } の AnkiConnect が応答しません。Anki は開いていますか？
error-anki-timeout = { $url } から時間内に応答がありません。ポートが別のプログラムに使われている可能性があります。
error-anki-http = { $url } の AnkiConnect が応答できませんでした（{ $reason }）。
error-anki-protocol = { $url } で応答しているのは AnkiConnect ではありません（{ $reason }）。
error-anki-wrong-key = AnkiConnect が API キーを拒否しました。Anki 設定で確認してください。
error-anki-unsupported = この AnkiConnect は { $action } には古すぎます。Anki で更新してください。
error-anki-no-collection = Anki でプロファイルが開かれていません。
error-anki-busy = Anki は処理中です。少し待ってから、もう一度お試しください。
error-anki-editor-open = Anki の「ブラウズ」「追加」「編集」ウィンドウを閉じてから、もう一度お試しください。
error-anki-model-missing = Anki にノートタイプ { $name } がありません。Anki 設定を更新してください。
error-anki-deck-missing = Anki にデッキ { $name } がありません。Anki 設定を更新してください。
error-anki-duplicate = このカードはすでに Anki にあります。
error-anki-empty-note = カードの最初のフィールドが空です。
error-anki-commit-unknown = Anki は { $action } を確認しませんでした。もう一度試す前に、カードが作成されたかを Anki で確認してください。
error-anki-lapis-missing = Anki にまだ Lapis のノートタイプまたはデッキがありません。Anki を確認してから、もう一度お試しください。
error-anki = AnkiConnect: { $reason }
error-link-invalid = これは Nikomiru の場面へのリンクではありません。
error-anki-unavailable = カードを作成できません。Anki の処理が開始されませんでした。
error-mining-dictionaries-changed = ポップアップを開いた後に辞書が変更されました。単語をもう一度検索してください。
error-mining-media = カードのメディアを作成できません（{ $reason }）。
error-later-keep = カードを後で送るために保存できません（{ $reason }）。
error-later-damaged = 後で送るカードを読み込めません（{ $reason }）。
error-mining-no-audio = カードは作成されませんでした。文の音声を切り出せません（動画に音声トラックが必要です）。
error-mining-mapping = ノートタイプのどのフィールドにもテンプレートがありません。Anki 設定でフィールドの割り当てを確認してください。
error-mining-note-gone = 最後のカードはもう Anki にありません。
error-mining-other-note-type = 最後のカードは { $name } のノートで、選択中のノートタイプではないため、上書きしませんでした。
error-mining-overwrite-lost = Anki は新しいフィールドを保持しませんでした。Anki のブラウザーでカードを閉じて、もう一度お試しください。
error-mining-overwrite-unknown = Anki は最後のカードの上書きを確認しませんでした。もう一度試す前に Anki で確認してください。
error-mining-not-found = Anki にこの単語のカードはありません。

# Online subtitles (nikomiru-online OnlineError, ADR 0008)
error-online-no-key = { $provider } の API キーがありません。設定 → 字幕サービスで追加してください。
error-online-bad-url = { $url } は Nikomiru が使用するアドレスではありません。HTTPS のみ使用できます。
error-online-unreachable = { $host } に接続できません（{ $reason }）。
error-online-timeout = { $host } から時間内に応答がありません。
error-online-unauthorized = { $provider } が API キーまたはログインを拒否しました。設定 → 字幕サービスで確認してください。
error-online-rate-limited = { $provider } へのリクエストが多すぎます。{ $seconds } 秒後にもう一度お試しください。
error-online-quota = 本日の { $provider } のダウンロード回数を使い切りました。
error-online-http = { $provider } が応答できませんでした（HTTP { $status }）。
error-online-gone = { $provider } にはこれはもうありません。
error-book-io = 本を開けません（{ $reason }）。
error-book-format = EPUB の本ではありません（{ $reason }）。
error-book-too-large = この本は大きすぎます。
error-book-no-text = この本にはテキストがありません。画像でできている可能性があります。
error-book-cbz = CBZ アーカイブではありません（{ $reason }）。
error-book-no-pages = このアーカイブには画像がありません。
error-ocr-model = 文字認識モデルを実行できません（{ $reason }）。
error-ocr-picture = このページを読み取れません（{ $reason }）。
error-ocr-small = 選択範囲が小さすぎます。
error-backup-io = バックアップを読み書きできませんでした: { $reason }
error-backup-library = ライブラリを読み込めませんでした: { $reason }
error-backup-damaged = Nikomiru のバックアップではないか、破損しています。
error-backup-newer = このバックアップは新しいバージョンの Nikomiru で作成されました。先に Nikomiru を更新してください。
error-backup-unsafe = このバックアップを安全に復元できません。
error-online-protocol = { $provider } から予期しない応答がありました（{ $reason }）。
error-online-too-large = ファイルが { $limit } MiB を超えています。
error-online-archive = アーカイブを使用できません（{ $reason }）。
error-online-unusable = { $name } は使用できません。SRT、ASS、SSA、WebVTT、ZIP ファイルのみ使用できます。
error-online-io = 字幕を保存できません（{ $reason }）。
error-online-cancelled = 中断しました。

# Single instance (nikomiru-app InstanceError)
error-instance-io = { $path } を使用できません（{ $reason }）。Nikomiru のウィンドウが複数開く可能性があります。
error-instance-no-answer = Nikomiru はすでに起動していますが、応答しません。閉じてから、もう一度お試しください。

# Subtitles (M2)
subtitles = 字幕
subtitles-off = オフ
subtitle-open-file = 字幕ファイルを開く…
subtitle-read-as-text = 文字として読む
subtitle-read-as-text-fetch = 文字として読む（{ $size } MB のモデルを一度だけダウンロード）
subtitle-search-online = オンラインで検索…
subtitle-align = 音声に合わせる
subtitle-secondary = サブ字幕
subtitle-secondary-shown = 表示
subtitle-secondary-blur = ポインターを重ねるまでぼかす
subtitle-secondary-hidden = ポインターを重ねるまで隠す
toast-sub-aligning = 字幕を音声に合わせています…
toast-sub-aligned = 字幕を音声に合わせました: { $value } ミリ秒（一致度 { $confidence }%）
toast-sub-align-unsure = 合わせられませんでした。音声と字幕がはっきり一致しません（最良 { $value } ミリ秒、一致度 { $confidence }%）
toast-sub-align-none = 合わせる対象がありません。字幕がないか、音声がありません
subtitle-encoding = 文字コード
encoding-auto = 自動
subtitle-list-short = リスト
subtitle-list = 字幕リスト
subtitle-search = 字幕を検索
subtitle-list-close = 字幕リストを閉じる
subtitle-score = 既知語: { $known }% · 未知語のない行: { $clear }%
subtitle-score-counting = 単語を数えています… { $done } / { $of } 行
subtitle-list-lines = 行
subtitle-list-words = 単語
subtitle-lines-all = すべての行
subtitle-lines-new = 新しい単語のある行
subtitle-lines-plus-one = 新しい単語が一つの行 (i+1)
subtitle-words-new-only = 未知の単語だけ
subtitle-words-all = 表示中をすべて
subtitle-words-ticked = { $count } 語を選択中:
subtitle-words-card = カード作成済み
library-score-counting = 字幕の単語を数えています…
file-filter-subtitles = 字幕ファイル
toast-sub-loaded = 字幕: { $name }
toast-sub-loaded-encoding = 字幕: { $name }（{ $encoding }）
toast-sub-reading = { $name } の画像を文字として読み取っています。終わると単語をクリックできます
toast-sub-read-fetching = { $name } の画像を読むため、読み取りモデル（{ $size } MB、初回のみ）をダウンロードしています
toast-sub-delay = 字幕のずれ { $value } ミリ秒
toast-sub-blurred = 字幕をぼかしました
toast-sub-hidden = 字幕を隠しました
toast-sub-visible = 字幕を表示しました
subtitles-disable = 字幕オフ
subtitles-auto-pause = 台詞の終わりごとに一時停止
subtitles-move = 字幕を移動
subtitles-move-hint = 字幕を上下にドラッグ · 終わったら Esc
toast-sub-off = 字幕オフ（V でもう一度表示）
toast-sub-on = 字幕オン
toast-auto-pause-on = 台詞ごとの一時停止：オン
toast-auto-pause-off = 台詞ごとの一時停止：オフ
encoding-utf-8 = Unicode — UTF-8
encoding-utf-16le = Unicode — UTF-16（リトルエンディアン）
encoding-utf-16be = Unicode — UTF-16（ビッグエンディアン）
encoding-cp932 = 日本語 — Shift_JIS（CP932）
encoding-euc-jp = 日本語 — EUC-JP
encoding-gb18030 = 簡体字中国語 — GB18030
encoding-big5 = 繁体字中国語 — Big5
encoding-cp1252 = 西ヨーロッパ — Windows-1252
encoding-iso-8859-1 = 西ヨーロッパ — ISO-8859-1
error-subtitles = 字幕を表示できません: { $reason }
error-subtitle-read = { $path } を読み込めません（{ $reason }）
error-subtitle-too-large = { $path } は字幕ファイルとしては大きすぎます（64 MiB 超）
error-subtitle-encoding = { $path } の文字コードを認識できません。字幕 → 文字コードで選択してください
error-subtitle-empty = ファイルに字幕の行がありません
error-subtitle-bitmap = 画像字幕はプレーヤーが描画します
error-subtitle-read-model = 読み取りモデルがインストールされていません
error-subtitle-read-failed = 画像を読み取れませんでした（{ $reason }）
error-subtitle-read-cancelled = 読み取りを中止しました

# Shown by the desktop while the screensaver is inhibited (PLY-013).
inhibit-reason = 動画を再生中

# Dictionary popup
popup-loading = 検索中…
popup-no-results = この単語はどの辞書にもありません。設定 → 辞書（📚）で辞書をインポートしてください。
popup-pitch-accent = ピッチアクセント
popup-replay = もう一度再生
popup-goto = この文へ移動
# The name of the popup's border for screen readers: dragging it resizes the popup.
popup-resize = ポップアップのサイズを変更
kanji-strokes = 画数: { $count }
kanji-radical = 部首: { $radical }

# Dictionaries page
dict-page-title = 辞書
dict-import = インポート…
dict-import-filter = 辞書（Yomitan、StarDict、DSL、MDict）
dict-empty = 辞書がまだありません。JMdict や Jitendex などの Yomitan 辞書（.zip）、頻度辞書、ピッチアクセント辞書をインポートするか、ウィンドウにドロップしてください。
dict-importing = インポート中
dict-cancel = キャンセル
dict-entries = 項目
dict-update-offer = この辞書の別のリビジョンがインストールされています。更新しますか？ 設定は保持されます。
dict-update = 更新
dict-keep = インストール済みのものを残す
dict-revision = リビジョン
dict-enabled = 検索に使用
dict-move-up = 上へ移動（優先度を上げる）
dict-move-down = 下へ移動
dict-rename = 名前を変更（空欄でタイトルに戻す）
dict-delete = 削除
dict-delete-confirm = この辞書を削除しますか？ ファイルも削除されます。
dict-collapse = ポップアップでの表示
dict-collapse-automatic = 自動
dict-collapse-expanded = 展開
dict-collapse-collapsed = 折りたたみ
dict-type-terms = 語句
dict-type-kanji = 漢字
dict-type-frequency = 頻度
dict-type-pitch = ピッチアクセント
dict-size-mb = { $whole }.{ $tenth } MB
dict-size-kb = { $size } kB
dict-imported = { $title } をインポートしました。
dict-same-revision = { $title } はこのリビジョンですでにインストールされています。
dict-import-busy = 実行中のインポートが終わるまでお待ちください。
dict-css-saved = { $title } のスタイルを保存しました。
dict-group-terms = 語句
dict-group-kanji = 漢字
dict-group-frequency = 頻度
dict-group-pitch = ピッチアクセント
dict-reference = { $name }（{ $group }に掲載）
dict-language = 言語 { $code }
dict-drag = ドラッグで順序を変更
dict-css = この辞書のスタイル（CSS）
dict-css-intro = この辞書の定義に使う独自の CSS です。辞書自身の styles.css の後に適用されます。使えるのは色、文字サイズ、太さとスタイル、マージン、パディング、枠線、背景のみで、Yomitan 辞書が使うセレクター（タグ、gloss-sc-… クラス、data-sc-… 属性）が対象です。
dict-css-field = { $title } の CSS
dict-css-save = 保存
dict-css-revert = 元に戻す
dict-css-clear = 消去
dict-css-unsaved = 未保存
dict-css-rules = 適用したルール: { $count }
dict-css-dropped = 除外したプロパティ: { $list }
dict-css-ignored = 無視したルール: { $list }
error-dict-unavailable = 辞書を使用できません。ライブラリを開けませんでした。
error-file-dialog = ファイルダイアログを使用できません。ファイルをウィンドウにドロップするか、コマンドラインで指定してください。

# Online subtitles page (MET-009, ADR 0008)
online-page-title = オンライン字幕
online-close = 閉じる
online-search-section = 検索
online-title = タイトル
online-season = シーズン
online-episode = 話
online-search = 検索
online-use = 使用
online-episode-n = 第 { $n } 話
online-archive = アーカイブ
online-unusable = 使用不可（RAR または 7z）
online-machine = 機械翻訳
online-services = サービスとキー…
services-list = サービス
services-list-hint = チェックしたサービスのみを検索します。同じくらい一致するファイルが二つある場合は、上にあるサービスが優先されます。
services-up = { $name } を上へ移動
services-down = { $name } を下へ移動
services-jimaku-about = コミュニティが投稿したアニメとドラマの日本語字幕です。無料の API キーが必要です。
services-os-about = 多くの言語の映画とシリーズの字幕で、ファイルのフィンガープリントでも探せます。API キーが必要で、ログインは任意です。
services-auto-section = 自動字幕
services-network = ネットワーク
services-metadata = シリーズのメタデータ
services-metadata-hint = ライブラリの「オンラインで識別」が、シリーズのタイトル、エピソード、カバーを問い合わせる先です。どちらもキーは不要です。識別したシリーズの字幕は、Jimaku が AniList の ID で探します。
online-auto = 優先する言語の字幕がない動画では、字幕を自動で検索して読み込む
online-jimaku-key = Jimaku の API キー
online-os-key = OpenSubtitles の API キー
online-os-user = OpenSubtitles のユーザー名（任意）
online-os-password = OpenSubtitles のパスワード（任意）
online-proxy = プロキシ（任意）
online-show-keys = キーとパスワードを表示
online-keys-hint = Jimaku のキーは jimaku.cc のアカウントページにあります。OpenSubtitles のキーは opensubtitles.com で作成するコンシューマーキーです（アカウントがあると 1 日のダウンロード数が増えます）。
online-save = 保存
online-no-video = 先に動画を開いてください。
online-downloading = { $name } をダウンロード中…
online-nothing-found = 字幕が見つかりません。別のタイトルにするか、話数を空にしてお試しください。
online-loaded = { $provider } の字幕: { $name }
online-auto-none = この動画のオンライン字幕は見つかりませんでした。
online-no-providers = 使用できる字幕サービスがありません。設定 → 字幕サービスで選択し、API キーを追加してください。

# Player settings page (PLY-016, PLY-018)
player-apply = 適用
player-apply-note = 「適用」は設定を保存し、オプションを今すぐ mpv に設定します。プロファイル、ハードウェアデコーダー、削除した raw オプションは、次のファイルまたは次の起動から反映されます。
player-presets = プリセット（mpv.conf）
player-preset-list = 保存したプリセット
player-load = 読み込む…
player-import = mpv.conf をインポート…
player-export = エクスポート…
player-preset-name = 名前
player-save-preset = プリセットとして保存
player-filter-conf = mpv の設定
player-preview = { $source } を読み込み中: 適用するまで何も変わりません。
player-line-control = コントロール
player-line-raw = raw オプション
player-preview-apply = 適用
player-preview-cancel = キャンセル
player-rendering = レンダリング
player-audio = 音声
player-advanced = 詳細
player-reset = リセット
player-default = （既定）
player-conf = 使用中の mpv.conf
player-conf-hint = Nikomiru が mpv に渡すオプションを mpv.conf の形式で示します。テキストまたは下のコントロールを編集してから適用してください。
player-user-conf = あなたの mpv.conf が見つかりました。「適用」で Nikomiru でも使用し、「キャンセル」で mpv の既定値のままにします。
player-raw-ignored = { $line } 行目: { $name } は適用されません。
player-raw-invalid = { $line } 行目: { $error }
player-ignored-fixed = Nikomiru が自動で設定します
player-ignored-blocked = 許可されていません: コードの実行、他のファイルの読み込み、ファイルの書き込みを行います
player-ignored-no-effect = Nikomiru では効果がありません
player-ignored-section = セクション内のプロファイルには対応していません
player-ignored-fit = 「一般」の「映像の表示」で設定します
player-later = 保存しました。次の項目は次のファイルまたは次の起動から反映されます: { $names }
player-refused = mpv が拒否しました: { $names }
player-read-error = { $path } を読み込めません（{ $reason }）。
player-write-error = { $path } に書き込めません（{ $reason }）。
player-bad-name = プリセット名に使えるのは文字、数字、スペース、- _ . のみです。
player-preset-saved = プリセット { $name } を保存しました。
player-exported = { $path } にエクスポートしました。

# Library page (LIB-002…LIB-012)
library-title = ライブラリ
library-close = 閉じる
library-add-folder = フォルダーを追加…
library-continue = 続きを見る
library-left = 残り { $time }
library-series = シリーズ
first-steps-title = はじめに
first-steps-hide = 隠す
first-steps-dictionary = 単語を検索するために、Yomitan 辞書（JMdict など）をインポートします。
first-steps-dictionary-button = 辞書…
first-steps-folder = シリーズや映画のフォルダーを追加します。
first-steps-card = 最初の Anki カードを作ります: AnkiConnect 入りの Anki を開き、ポップアップの + を押します。
first-steps-card-button = Anki 設定…
library-empty = 動画がまだありません。シリーズや映画のフォルダーを追加してください。
library-watched = { $total } 本中 { $watched } 本を視聴済み
library-folders = フォルダー
library-grouping = グループ化
library-by-series = シリーズ別
library-by-folder = フォルダー別
library-rescan = 再スキャン
library-back = ← ライブラリ
library-name = 名前
library-rename = 名前を変更
library-merge-into = 統合先
library-merge = 統合
library-delete = ライブラリから削除（ファイルは残ります）
library-episodes = エピソード（{ $count }）
library-play = 再生
library-move-to = 移動先
library-move = 移動
library-split-name = チェックしたエピソードから新しいシリーズを作成
library-split = 作成
library-remove = チェックしたエピソードをライブラリから削除（ファイルは残ります）
library-scanning = { $path } の動画を探しています…
library-imported = 新しい動画 { $added } 本、新しいシリーズ { $series } 件。
library-no-folder = { $path } は読み取れるフォルダーではありません。
library-identify = オンラインで識別
library-identify-hint = このシリーズのタイトル、エピソード、カバーを AniList（設定 → 字幕サービスで選べば MyAnimeList）に問い合わせます。これ以外でオンラインに送信されるものはありません。
library-identifying = 問い合わせ中…
library-identify-choose = { $source } に完全に同じタイトルのシリーズはありません。どれですか？
library-identify-pick = これ
library-identify-this = これ: { $title }
library-identify-none-of-these = どれでもない
library-identify-none = { $source } でこのタイトルは見つかりませんでした。シリーズの名前を変更して、もう一度お試しください。
library-identified = { $title } として識別しました。
library-identify-again = もう一度識別
library-forget = 情報を消去
library-online-page = 開く:
library-online-episodes = 全 { $count } 話

# Next episode (LIB-009)
next-up = { $seconds } 秒後に次の話
next-up-play = 今すぐ再生
next-up-cancel = キャンセル

# Word audio (AUD-001…AUD-008)
word-audio-button = 単語の音声
word-audio-hint = 単語を再生、右クリックですべてのソース
word-audio-loading = ソースに問い合わせ中…
word-audio-none = この単語の音声はありません。
word-audio-unavailable = 単語の音声を再生できません。ログを確認してください。

# Word audio tab (AUD-001, AUD-005, AUD-006)
audio-save = 保存
audio-sources = ソース（問い合わせる順）
audio-sources-hint = ソースは {"{"}term{"}"} と {"{"}reading{"}"} を含むアドレスです。たとえば Yomitan 用のローカル音声サーバーのように、Yomitan の音声ファイルリストまたは音声そのものを返すもの（HTTPS のみ、このコンピューターでは HTTP も可）です。または Local Audio Server for Yomichan の android.db をディスクから読み込みます。組み込みのソースはありません。
audio-source-n = ソース { $n }
audio-name = 名前
audio-url = アドレス
audio-up = ソース { $n } を上へ移動
audio-down = ソース { $n } を下へ移動
audio-remove = ソース { $n } を削除
audio-add = ソースを追加
audio-playing = 再生
audio-autoplay = ポップアップが開いたら単語を自動で再生する
audio-volume = 音量
audio-database = ソース { $n } のデータベースを選択
audio-database-button = データベース…
audio-database-filter = 音声データベース
audio-priority = サブソース（順番どおり）
word-audio-database = 音声データベース { $path } を使用できません（{ $reason }）。

# Statistics (STA-005)
stats-title = 統計
stats-export = CSV をエクスポート…
stats-csv-filter = CSV ファイル
stats-exported = セッションは { $path } にあります。
stats-under-minute = 1 分未満
stats-minutes = { $m } 分
stats-hours = { $h } 時間 { $m } 分
stats-today = 今日: { $time } · { $chars } 文字
stats-week = 7 日間: { $time } · { $chars } 文字
stats-all = 合計: { $time } · { $chars } 文字
stats-chart = 日ごとの視聴時間
stats-range = { $n } 日間
stats-day = { $date }: { $time } · { $chars } 文字
stats-calendar = 1 年間
stats-series = シリーズ別
stats-time-chars = { $time } · { $chars } 文字
stats-sessions = セッション
stats-none = セッションはまだありません。視聴中に記録されます。
stats-session = { $time } · { $chars } 文字 · 検索 { $lookups ->
   *[other] { $lookups } 回
} · カード { $cards } 枚
stats-delete = 削除
stats-delete-session = { $date } の { $name } のセッションを削除

settings-servers = メディアサーバー
servers-added = 追加済みのサーバー
servers-none = サーバーはまだありません。Nikomiru は追加されたサーバーにのみ接続します。
servers-add-jellyfin = Jellyfin サーバーを追加
servers-name = 名前
servers-address = アドレス
servers-address-hint = http://192.168.1.10:8096
servers-user = ユーザー
servers-password = パスワード
servers-unencrypted = 暗号化されていない接続を許可
servers-unencrypted-warning = パスワードとトークンが平文で送信されます。自分のネットワーク外の http:// アドレスにのみ必要です。
servers-log-in = ログイン
servers-add-plex = Plex サーバーを追加
servers-plex-hint = ブラウザーで plex.tv にサインインします。パスワードは Nikomiru を経由しません。
servers-sign-in-plex = Plex でサインイン
servers-cancel = キャンセル
servers-plex-stopped = Plex のサインインは完了前に中断されました。
servers-check = ライブラリ
servers-remove = 削除
servers-added-count = サーバーを { $count } 件追加しました。
servers-this-server = このサーバー
servers-search = このライブラリを検索
servers-empty = 何もありません。
servers-more = もっと見る
servers-watched = 視聴済み
servers-started = 視聴中

settings-backup = バックアップ
backup-file = ファイルにバックアップ
backup-file-hint = 設定、ライブラリ、進捗、統計、ダウンロードした字幕を 1 つのファイルにまとめます。
backup-dictionaries = 辞書を含める（大容量）
backup-secrets = キーとパスワードを含める
backup-secrets-warning = ファイルに平文で保存されます。安全な場所に保管してください。
backup-make = バックアップ…
backup-restore = 復元…
backup-filter = Nikomiru のバックアップ
backup-restore-confirm = { $date } に Nikomiru { $version } で作成したバックアップです。次の起動時に設定、ライブラリ、統計を置き換えます。現在のものは before-restore に保存されます。
backup-restore-yes = 次の起動時に復元
backup-done = バックアップを書き込みました。
backup-restored = バックアップの準備ができました。Nikomiru を閉じて開き直すと復元されます。
sync-title = WebDAV で同期
sync-hint = 自分のサーバー上のフォルダー（Nextcloud など、アプリパスワードを使用）を通じて、進捗、統計、カードをコンピューター間でそろえます。設定、シリーズ、辞書はそれぞれのコンピューターに残ります。サーバー上のファイルは暗号化されません。
sync-automatic = 起動時と終了時に同期
sync-now = 今すぐ同期
sync-saved = 保存しました。
sync-checked = このフォルダーに書き込めます。
sync-done = 他のデバイス { $devices } 台と同期しました。{ $skipped } 件をスキップしました。

chapters = チャプター
chapter-n = チャプター { $n }
word-audio-setup = 発音のソースがまだありません。設定で追加してください
hooker-title = テキストフッカー
hooker-connected = 接続済み
hooker-waiting = フッカーを待っています…
hooker-clear = 消去
hooker-clipboard = クリップボードも読み取る
hooker-hint = Textractor や LunaTranslator の WebSocket から届いた行がここに表示されます。単語をクリックして検索し、ポップアップからカードを作成できます（文のみで、画像と音声はありません）。Ctrl+V でコピーした内容を追加します。バックグラウンドでのクリップボードの読み取りには wl-paste または xclip が必要で、GNOME Wayland では動作しません。日本語のテキストのみ保持します。
hooker-no-clipboard = クリップボードを読み取るプログラムがありません。wl-paste または xclip をインストールしてください。

books-title = 本
books-add = 本またはマンガを追加…
books-folder = ページのフォルダーを追加…
books-none = 本がまだありません。EPUB、漫画、HTML、TXT、Markdown、HTMLZ、RTF、DOCX、テキスト付きPDFを追加するか、テキストを貼り付けるか、アドレスからウェブページを開いてください。元のファイルは移動しません。
books-audiobook = オーディオブックを開く…
books-audio-filter = 音声ファイル
books-filter = 本・マンガ・HTML/TXT/Markdown・Mokuro・HTMLZ/RTF/DOCX/PDF
books-remove = { $title } を削除
books-back = ← 本
books-vertical = 縦書き
books-next = 次の章
books-read = 読む
books-score-analyse = 分析
books-score-refresh = 更新
books-score-analyse-title = 「{ $title }」の理解度を分析
books-score-counting = 本を分析中…
books-score-summary = 理解度：{ $known }% · { $clear }%
books-score-detail = 本全体で、既知の単語の出現割合は { $known }%、新しい単語のない段落は { $clear }%
books-score-no-text = 分析できる本文がありません
books-score-too-long = 本全体の分析上限を超えています
books-score-failed = 分析できません
books-score-wait = 別の本を分析中です
books-score-source-changed = 本のファイルが変更されています。一度開いてライブラリの情報を更新してから、もう一度分析してください。
books-words = 単語
books-words-search = 単語や読み方を検索
books-chapter-score = この章：既知の単語 { $known }% · 新しい単語のない段落 { $clear }%
books-chapter-counting = この章を集計中：{ $of } 段落中 { $done } 段落
books-chapter-too-long = この章は長すぎるため集計できません。そのまま読み進められます。
books-previous = 前の章
books-next-page = 次のページ
books-previous-page = 前のページ
manga-double = 見開き
manga-page = { $number } ページ
manga-bad-page = このページを表示できません。
manga-offer = ページの文字を読み取るには、{ $size } MB の文字認識モデルが必要です。Hugging Face から一度だけダウンロードします。処理はこのコンピューター上で行われ、ページが外部に送信されることはありません。
manga-credit = モデル: kha-white の manga-ocr（Apache-2.0）、Manga109-s データセットで学習。
manga-fetch = モデルをダウンロード
manga-fetching = モデルをダウンロード中: { $percent }%
manga-hint = フキダシの上をドラッグすると読み取ります。
manga-reading = 読み取り中…
manga-nothing = そこに文字は見つかりませんでした。

# Review (REV-001…REV-006)
review-title = 復習
review-title-due = 復習（{ $count }）
review-deck = Nikomiru の復習
review-in-library = このカードは Nikomiru の復習にあります：ライブラリ → 復習。
error-review-no-library = 復習のカードにはライブラリが必要ですが、開けませんでした。
review-count-new = 新規：{ $count }
review-count-learning = 学習中：{ $count }
review-count-due = 復習：{ $count }
review-reload = 再読み込み
review-undo = 元に戻す
review-done = 今は復習するカードがありません。
review-next = 次のカードは{ $when }後に戻ります。
review-total = { $count ->
    [one] 復習のカードは { $count } 枚です。
   *[other] 復習のカードは { $count } 枚です。
}
review-empty-hint = Anki ではなくここでカードを作るには、設定 → Anki → カードの保存先 で「Nikomiru の復習」を選びます。その後、ポップアップのカードボタンで追加されます。
review-show = 答えを見る
review-show-keys = Space または Enter、その後 1–4 で回答
review-sentence-audio = ▶ 文
review-word-audio = ♪ 単語
review-scene = 場面を開く
review-delete = 削除
review-delete-confirm = 削除しますか？もう一度クリック
review-again = もう一度
review-hard = 難しい
review-good = 正解
review-easy = 簡単
review-minutes = { $n }分
review-hours = { $n }時間
review-days = { $n }日
review-months = { $n }か月
review-years = { $n }年
anki-destination = カードの保存先
anki-destination-anki = Anki
anki-destination-nikomiru = Nikomiru の復習
anki-destination-later = Anki（後で送る）
anki-destination-later-hint = カードは音声と画像付きですぐに作られ、このコンピューターに保存されます。Anki を開いていなくても構いません。開いたら下から送ってください。
anki-later = 後で送るカード（{ $count }）
anki-later-none = 保存されたカードはありません。
anki-later-remove = 削除
anki-later-damaged = 読み込めないカードです。削除してください。
anki-later-send = Anki に送る
anki-destination-hint = Nikomiru の復習はカードをライブラリに保存し、ライブラリページの「復習」タブで FSRS により出題します。Anki は不要です。
review-settings = Nikomiru での復習
review-retention = カードが戻ってきたときに覚えている割合
review-new-per-day = 1日の新規カード数
books-web = ウェブページ
books-web-open = 開く
error-book-not-page = このページは読めません：Nikomiru が知らない文字コードで書かれています。
stats-words = 単語
stats-words-set = 既知: { $known } · 学習中: { $learning } · 無視: { $ignored }
stats-words-anki = Ankiから: 既知 { $known }、学習中 { $learning }
stats-words-days = 既知の単語（過去30日）
stats-words-day = { $ago ->
    [0] 今日
    [1] 昨日
   *[other] { $ago }日前
}: 既知 { $count } 語
stats-review = 復習
stats-review-today = 今日：回答 { $count } 件、覚えていた { $kept } 件
stats-review-retention = 復習で覚えていた割合（過去30日）：{ $percent } %
stats-review-retention-none = 復習で覚えていた割合（過去30日）：まだ回答がありません
stats-review-cards = 新規 { $new } · 学習中 { $learning } · 若い { $young } · 成熟 { $mature }
stats-review-days = 1日の回答数（過去30日）
stats-review-day = { $ago ->
    [0] 今日
    [1] 昨日
   *[other] { $ago }日前
}：回答 { $count } 件
stats-review-ahead = 期限のカード（今後7日）
stats-review-due = { $in ->
    [0] 今日
    [1] 明日
   *[other] { $in }日後
}：{ $count } 枚
review-study = 学習
review-cards = カード（{ $count }）
review-search = 検索
review-no-match = 該当するカードはありません。
review-shown = { $count } 枚のうち最初の { $shown } 枚：ほかは検索で探してください。
review-phase-new = 新規
review-phase-learning = 学習中
review-phase-review = 復習中
review-phase-relearning = 再学習中
review-due-now = 今が期限
review-answers = { $reps ->
   *[other] 回答 { $reps } 回
}、忘れた回数 { $lapses } 回
review-reset = やり直す
review-reset-confirm = やり直しますか？もう一度クリック
books-along = 聞きながら読む
books-along-hint = 字幕付きのオーディオブックを再生してから、その本に戻ってください。ページが聞こえている箇所を追います。
books-along-none = 字幕付きのものが再生されていません。先にオーディオブックを開いてください。
books-along-not-found = オーディオブックの行がこの本に見つかりません。
books-along-found = オーディオブックの { $count } 行のうち { $found } 行が本に見つかりました。
books-listen = ここから聞く

# Setup assistant (INS-001…INS-006)
setup-title = セットアップアシスタント
setup-close = 閉じる
setup-open = 辞書・音声・カード・ツールを設定…
setup-open-hint = 無料の辞書、単語の音声、補助ツールをダウンロードし、カードの作り方を設定します。
setup-first-steps = 辞書・単語の音声・カード・補助ツールをまとめて設定：
setup-intro = インストールするものを選んでください。「インストール」を押すまで何もダウンロードしません。ダウンロードは Nikomiru を使っている間も続きます。未完了の補助ツールのダウンロードは、再試行時に最初から開始します。
setup-dictionaries = 辞書
setup-dictionaries-hint = 作者が公開している場所からダウンロードし、手動で選んだファイルと同じようにインポートします。
setup-license-unstated = 配布元の条件（明記なし）
setup-audio = 単語の音声
setup-audio-hint = Fushi の推奨パックから、チェックしたライブラリだけをダウンロードします（パックの辞書は含みません）。パックは Fushi の作者によるもので、録音のライセンスは明記されていません。
setup-audio-japanese = 日本語の発音
setup-audio-english = 英語の発音
setup-install = インストール
setup-stop = 停止
setup-cards = カード
setup-cards-anki = Anki
setup-cards-review = Nikomiru の復習
setup-anki-answers = Anki が応答しました（AnkiConnect { $version }）。
setup-anki-lapis-in-use = カードは Lapis ノートタイプを使います。
setup-anki-use-lapis = Lapis ノートタイプを使う
setup-anki-away = Anki が応答しません。
setup-anki-steps = apps.ankiweb.net から Anki をインストールし、Anki で「ツール → アドオン → アドオンを取得…」にコード 2055492159（AnkiConnect）を入力して Anki を再起動してください。カードを作る間は Anki を開いたままにしてください。
setup-anki-retry = 再試行
setup-anki-later = その間はカードを後で送るために保存
setup-state-installed = インストール済み
setup-state-waiting = 待機中
setup-state-downloading = { $size } の { $percent } %
setup-state-importing = インポート中
setup-state-done = 完了
setup-finished = セットアップアシスタントが完了しました。
error-setup-space = 空き容量が足りません：{ $need } 必要、空き { $free }。
error-setup-no-library = パックに { $name } がありません。
error-setup-not-audio = ダウンロードしたファイルは単語音声のデータベースではありません（{ $reason }）。

# AUD-010
condense-export = 台詞の音声を書き出す
condense-folder = 音声の保存先フォルダー
condense-hint = 各話を1つの音声ファイルにし、字幕の行ごとにチャプターを付けます。各話に保存された字幕と音声の選択を使います。
condense-running = 台詞の音声を書き出しています…
condense-cancel = 音声の書き出しを中止
condense-cancelled = 書き出しを中止しました。保存済み: { $count }、失敗した話: { $failed }。
condense-done = 保存した音声: { $count }、失敗した話: { $failed }。
condense-no-subtitles = 使用できるテキスト字幕がありません。テキスト字幕を選ぶか、画像字幕を先に読み取ってください。
condense-no-audio = この話には書き出せる音声トラックがありません。
condense-local-only = シリーズの音声を書き出すにはローカルの動画が必要です。
condense-failed = 音声の書き出しに失敗しました: { $reason }

# PLY-023
webvideo-open = ウェブの動画を開く
webvideo-address = 動画ページのアドレス
webvideo-hint = パソコンにインストールしたyt-dlpで動画ページを開きます。テキスト字幕の単語を調べ、カードを作成できます。
webvideo-cancel = キャンセル
webvideo-play = 動画を開く
webvideo-opening = ウェブの動画を開いています…
webvideo-unavailable = ウェブ動画の処理を開始できませんでした。ローカルファイルは再生できます。
webvideo-missing = yt-dlpが見つかりません。インストールするか、設定 → 詳細で実行ファイルを指定してください。
webvideo-failed = 動画を開けませんでした。アドレス、動画の公開状況、yt-dlpが最新かどうかを確認してください。
webvideo-stream = 対応するメディアがありません。ライブ、DRM保護、分割形式のみの動画には対応していません。
webvideo-invalid = 動画ページの有効なHTTPSアドレスを入力してください。
webvideo-cancelled = ウェブ動画を開く処理を中止しました。
settings-yt-dlp = yt-dlpの実行ファイル
settings-yt-dlp-hint = 空欄: NIKOMIRU_YT_DLP、次にPATH

# KNW-017: explicit series comprehension
library-score-analyse = 分析
library-score-refresh = 更新
library-score-analyse-title = 「{ $title }」の理解度を分析
library-score-loading = エピソード一覧を読み込み中…
library-score-counting = エピソードを分析中：{ $done } / { $total }
library-score-summary = 理解度：{ $known }% · { $clear }%
library-score-detail = 分析したエピソードで、既知の単語の出現割合は { $known }%、新しい単語のない行は { $clear }%
library-score-coverage = { $total } 話中 { $scored } 話を分析 · { $failed } 話で失敗
library-score-unavailable = 分析できる字幕がありません
library-score-too-large = エピソード数が分析上限を超えています

# KNW-018: historical playback encounters
stats-encounters = エピソードで出会った新しい単語
stats-encounters-detail = 再生中に出会った時点で新しかった単語の異なり数です。後で状態を変えても、この履歴は残ります。
stats-encounters-episode = { $title }：新しい単語 { $count } 語

condense-reading = 字幕の読み取りが終わってから音声をまとめて書き出してください。

error-subtitle-vobsub-index = VobSub のインデックスが破損しているか、読み取れる言語トラックがありません。

analysis-cancel = 解析をキャンセル

analysis-words = 学習する単語
analysis-words-back = 一覧に戻る

analysis-export-shown = 表示中の単語を出力
analysis-export-selected = 選択した単語を出力
analysis-export-saved = { $path } に単語を保存しました。
analysis-export-too-large = 単語一覧が出力上限の16 MiBを超えています。

encounter-history-open = 履歴を見る
encounter-history-clear = エピソードの履歴を消去
encounter-history-coverage = 履歴の { $total } 語中 { $shown } 語を表示（初遭遇：Unixミリ秒）。

analysis-order = 理解度の並び順
analysis-order-original = ライブラリ順
analysis-order-most = 既知の割合が高い順
analysis-order-least = 既知の割合が低い順
analysis-only-scored = 分析済みのみ
analysis-minimum-known = 既知の割合の下限 %

analysis-source-changed = 分析中にソースまたは辞書が変更されました。再分析してください。

vocabulary-status-filter = 単語の状態フィルター
vocabulary-all-statuses = すべての状態
vocabulary-card-filter = カードフィルター
vocabulary-any-card = カードの有無を問わない
vocabulary-with-card = カードあり
vocabulary-without-card = カードなし

vocabulary-order = 単語の並び順
vocabulary-frequency-desc = 頻度が高い順
vocabulary-frequency-asc = 頻度が低い順
vocabulary-expression = 表記順
vocabulary-reading = 読み順

vocabulary-copy-shown = 表示中の単語をコピー
vocabulary-copy-selected = 選択した単語をコピー
vocabulary-copied = 単語をコピーしました。

encounter-export = エピソードの全履歴を出力
encounter-export-saved = エピソードの履歴を { $path } に保存しました。

episode-words-title = 学習する単語：{ $title }

sub-reading-preparing = 字幕の読み取りを準備中…
sub-reading-progress = { $total } 枚中 { $done } 枚を読み取り
sub-reading-cancel = 読み取りをキャンセル

sub-export = 現在の字幕をSRTで出力…
sub-export-filter = SubRip字幕
sub-export-unavailable = 出力には完全で空でないテキスト字幕が必要です。
sub-export-too-large = 字幕が出力上限の16 MiBを超えています。
sub-export-saved = 字幕を { $path } に保存しました。

sub-export-failed = 字幕を保存できませんでした：{ $reason }

analysis-cache-title = 分析カウントのキャッシュ
analysis-cache-usage = { $files } ファイル · { $bytes } バイト
analysis-cache-unavailable = キャッシュ使用量を取得できません。
analysis-cache-refresh = 使用量を更新
analysis-cache-clear = カウントキャッシュを消去
analysis-cache-detail = 本・エピソードの派生カウントは最大64 MiBです。現在の分析は維持され、完了した処理によりキャッシュが再作成されることがあります。

study-report-export = 分析済みの概要を出力…
study-report-empty = このレポートに使える完了済みの分析がありません。
study-report-saved = 理解度レポートを { $path } に保存しました。
study-report-failed = レポートを保存できませんでした：{ $reason }

# 選択した補助ツール (INS-007)。
setup-tools = 補助ツール
setup-tools-hint = インストールするツールを選択してください。動作する既存のツールは再利用します。yt-dlp は FFmpeg と Deno も必要です。ダウンロードを検証し、管理者権限なしでこのアプリ用にインストールします。
setup-runtime-ready = 再生・描画・TLS ライブラリはアプリのパッケージまたは OS が提供します。
setup-tools-refresh = インストール済みツールを確認
setup-tool-required = 選択したツールに必要
setup-tool-available = ローカルで利用可能・再利用します
setup-tool-model = ローカル OCR モデル
setup-tool-checking = 展開・検証中…
setup-tool-unsupported = このプラットフォーム用のインストーラーはありません
setup-tool-cancelled = インストールをキャンセルしました
setup-tool-invalid = ツールまたはインストール計画が無効です。
setup-tool-missing = ツールが見つかりません。
setup-tool-archive = アーカイブが不完全か、安全でない項目を含んでいます。
setup-tool-too-large = ダウンロードまたは展開後のファイルがサイズ制限を超えています。
setup-tool-damaged = インストール先を安全に置き換えられません。
setup-tool-space = インストールに必要な空き容量がありません。
setup-tool-dependency = 必要なコンポーネントをインストールできませんでした。
setup-tool-failed = インストールに失敗しました: { $reason }
settings-deno = Deno の実行ファイルまたはフォルダー

# Subtitle study selections (SUB-035).
sub-study-tools = 学習ツール
sub-select-shown = 表示中の行を選択
sub-selection-clear = 行の選択を解除
sub-export-selected = 選択した行を SRT で保存…
sub-select-line = 行を選択

sub-copy-shown = 表示中の書き起こしをコピー
sub-copy-selected = 選択した書き起こしをコピー
sub-transcript-copied = 書き起こしをコピーしました

sub-vtt-shown = 表示中の行を WebVTT で保存…
sub-vtt-selected = 選択した行を WebVTT で保存…
sub-vtt-filter = WebVTT 字幕

sub-bilingual-shown = 表示中の対訳を書き出す…
sub-bilingual-selected = 選択した対訳を書き出す…

sub-window-from = 現在位置から
sub-window-to = 現在位置まで
sub-window-reset = 時間範囲をリセット

vocabulary-min-count = 最小出現回数
vocabulary-max-count = 最大出現回数
vocabulary-reset-counts = 出現回数の制限をリセット

vocabulary-shown-words = 表示中の語数
vocabulary-selected-words = 選択した語数
vocabulary-occurrences = 出現回数

vocabulary-lookup = 辞書で調べる

vocabulary-examples = 字幕内の用例
vocabulary-examples-clear = 用例フィルターを解除
vocabulary-examples-unavailable = 現在の字幕の解析が完了するまで用例は利用できません。

frequency-export-shown = 表示中の頻度辞書を書き出す…
frequency-export-selected = 選択した頻度辞書を書き出す…
frequency-export-invalid = 空でない一意の語と対応する正の出現回数が必要です。

vocabulary-exact = 表記・読みの完全一致

sub-speed-tools = 読速（文字/秒）
sub-speed-min = 最小文字数/秒
sub-speed-max = 最大文字数/秒
sub-speed-reset = 読速の制限をリセット

sub-stats-title = 範囲の統計
sub-stats-shown = 表示中
sub-stats-selected = 選択中
sub-stats-lines = 行
sub-stats-characters = 文字
sub-stats-dialogue = 発話時間

sub-bilingual-srt-shown = 表示中の対訳を SRT で保存…
sub-bilingual-srt-selected = 選択した対訳を SRT で保存…

sub-previous-match = 前の一致行
sub-next-match = 次の一致行

vocabulary-presets = 再利用できる学習フィルター
vocabulary-preset-save = フィルターを記憶
vocabulary-preset-apply = 記憶したフィルターを適用
vocabulary-preset-clear = フィルターを忘れる
vocabulary-preset-hint = このセッション中のみ記憶します。語の選択は各画面に保持します。
vocabulary-preset-too-long = 検索が長すぎます（4096 バイトまで）。

vocabulary-named-presets = 学習プリセット
vocabulary-preset-name = プリセット名
vocabulary-preset-store = プリセットを保存
vocabulary-preset-delete = プリセットを削除

vocabulary-script-any = すべての文字種
vocabulary-script-kanji = 漢字を含む
vocabulary-script-kana = かなを含む
vocabulary-script-latin = ラテン文字を含む

vocabulary-length-min = 見出し語の最小文字数
vocabulary-length-max = 見出し語の最大文字数
vocabulary-length-reset = 文字数をリセット

sub-duration-tools = 字幕の長さ
sub-duration-min = 最短時間（ミリ秒）
sub-duration-max = 最長時間（ミリ秒）
sub-duration-reset = 時間条件をリセット

sub-mask-text = 聞き取り練習のため字幕を隠す
sub-reveal-shown = 表示範囲の字幕を開く
sub-mask-again = 字幕を再び隠す
sub-reveal-line = 字幕を開く

sub-vocabulary-shown = 表示中の字幕の語彙を学習
sub-vocabulary-selected = 選択した字幕の語彙を学習
sub-vocabulary-all = 字幕全体の語彙に戻る

sub-html-shown = 表示範囲をHTML学習シートに出力
sub-html-selected = 選択範囲をHTML学習シートに出力
sub-html-translation = 翻訳を表示
sub-html-filter = HTML学習シート

sub-invert-shown = 表示範囲の選択を反転

vocabulary-batch-tools = 語彙学習グループ
vocabulary-batch-next = 次のグループを選択
vocabulary-batch-clear = 表示範囲の選択を解除
vocabulary-batch-invert = 表示範囲の選択を反転
vocabulary-batch-size = グループの語数

sub-practice-current = 現在の字幕を繰り返す
sub-practice-stop = 繰り返しを停止
sub-practice-remaining = 残りの再生回数：{ $count }

sub-practice-tools = 字幕の繰り返し練習
sub-vocabulary-tools = この範囲の語彙
sub-html-tools = HTML学習シート

sub-text-tools = 字幕テキストの条件
sub-text-min = 最小文字数
sub-text-max = 最大文字数
sub-text-reset = 文字数条件をリセット

sub-unique-captions = 同じ字幕は最初の出現だけ表示

sub-shape-any = 行数の指定なし
sub-shape-single = 一行の字幕
sub-shape-multiple = 複数行の字幕

vocabulary-reading-tools = 読みとカバー率の条件
vocabulary-reading-any = すべての読み
vocabulary-reading-distinct = 異なる読みがある語
vocabulary-reading-same = 異なる読みがない語

vocabulary-coverage-target = 出現回数の指定割合をカバーする頻出語
vocabulary-coverage-reset = カバー率の条件をリセット

sub-presets = 保存した字幕学習プリセット

vocabulary-html-tools = 語彙のHTML学習シート
vocabulary-html-shown = 表示中の語彙をHTMLに出力
vocabulary-html-selected = 選択した語彙をHTMLに出力
vocabulary-html-reading = 読みを表示

sub-context-exact = 一致する字幕だけ
sub-context-one = 前後一字幕
sub-context-two = 前後二字幕
sub-context-hit = 単語が一致する字幕

action-study-previous = 前の一致する字幕
action-study-next = 次の一致する字幕
action-study-reveal = 現在の学習字幕を表示
action-study-select = 現在の学習字幕の選択を切り替え

sub-practice-shown = 表示中の字幕を繰り返す
sub-practice-selected = 選択した字幕を繰り返す
sub-practice-passes = 字幕ごとの再生回数
sub-practice-cues = 残りの字幕数：{ $count }
sub-practice-unavailable = 動画の範囲内にある有効な字幕を1～1000件選んでください。

sub-search-exact = 字幕全体と一致

sub-order-source = 元の字幕順
sub-order-short = 短い時間から
sub-order-long = 長い時間から
sub-order-text = 短い文章から

sub-time-edit = 学習時間の範囲を入力
sub-time-from = 開始（秒またはH:MM:SS.mmm）
sub-time-to = 終了（秒またはH:MM:SS.mmm）
sub-time-apply = 時間範囲を適用
sub-time-invalid = 終了は開始より後にしてください。空欄は制限なしです。

sub-copy-export = コピーと書き出し

sub-preview-title = 字幕の翻訳
sub-preview-show = この字幕の翻訳を表示
sub-preview-empty = 重なる翻訳はありません。

vocabulary-practice-mask = 読みから単語を思い出す練習
vocabulary-practice-reveal = 表示範囲の単語を開く
vocabulary-practice-hide = 単語を再び隠す
vocabulary-practice-word = 単語を表示

vocabulary-practice-select = 学習する単語を選択

vocabulary-undo-status = 単語の状態を元に戻す
vocabulary-undo-unavailable = 前の状態変更は元に戻せません。

sub-practice-pause = 繰り返しの間の休止（ミリ秒）
sub-practice-wait = 待機中：{ $count } ミリ秒

sub-practice-skip = 現在の字幕をスキップ
sub-practice-restart = 現在の字幕を最初から繰り返す

vocabulary-examples-export-shown = 表示中の単語の例文を出力
vocabulary-examples-export-selected = 選択した単語の例文を出力

study-presets-import = 学習プリセットを読み込む…
study-presets-export = 学習プリセットを書き出す…
study-presets-filter = 学習プリセット（JSON）
study-presets-invalid = 無効なファイルか、対応していない条件が含まれています。
study-presets-too-large = ファイルまたは統合後の件数が上限を超えています。
study-presets-unsupported = このプリセット形式のバージョンには対応していません。
study-presets-import-failed = プリセットを読み書きできません：{ $reason }

recall-title = 語彙の想起練習
recall-reveal = 答えを表示
recall-skip = 単語をスキップ
recall-complete = ラウンド完了
recall-no-reading = 異なる読みはありません。準備ができたら単語を表示してください。
recall-remaining = 残りの単語
recall-start-shown = 表示中の語彙で練習
recall-start-selected = 選択した語彙で練習
recall-unavailable = 有効な見出し語と読みを持つ単語を1～1000件選んでください。

recall-again = もう一度
recall-good = できた
recall-good-total = できた単語
recall-again-total = もう一度の評価数
recall-skipped-total = スキップした単語
recall-attempts-total = 回答回数

action-recall-reveal = 想起練習の答えを表示
action-recall-again = 想起練習：もう一度
action-recall-good = 想起練習：できた

shortcuts-recall = 語彙の想起練習

recall-hints-capture = 現在の字幕から例文を取得
recall-hints-reveal = 例文のヒントを表示
recall-hints-unavailable = 解析済みの主字幕と、上限内の例文が必要です。

recall-restart-original = 元の順序でやり直す
recall-restart-reverse = 逆順でやり直す
recall-restart-shuffle = 順序を混ぜてやり直す

recall-save = 想起練習の進捗を保存…
recall-load = 保存した想起練習を開く…
recall-file-filter = 想起練習の進捗（JSON）
recall-file-invalid = ファイルが無効か、進捗の整合性がありません。
recall-file-too-large = 想起練習ファイルが上限を超えています。
recall-file-version = この想起練習ファイルのバージョンには対応していません。
recall-file-error = 進捗を読み書きできません：{ $reason }

recall-results-export = 想起練習の結果を書き出す…

recall-followup-difficult = 難しかった単語で新しい練習
recall-followup-skipped = スキップした単語で新しい練習
recall-followup-unavailable = ラウンドを完了して、単語がある分類を選んでください。

recall-history = 想起練習の履歴
recall-history-record = この集計を記録
recall-history-clear = 保存した集計をすべて削除
recall-history-remove = 集計を削除
recall-history-empty = 保存された集計はありません。
recall-history-words = 単語数
recall-history-error = 履歴を保存できません：{ $reason }
recall-history-unsaved = 履歴の最新の変更は保存されていません。
recall-history-retry = 保存を再試行

books-paste = テキストを貼り付け…
books-paste-title = タイトル
books-paste-content = テキスト
books-paste-markdown = Markdown として読む
books-paste-read = テキストを読む
books-paste-hint = 再度開けるようにローカルに保存します。上限 4 MiB。Markdown の見出しとルビを保持し、リンクや画像は開きません。
error-book-invalid-text = このテキスト文書は読めません。UTF-8、256 文字以内のタイトル、プレーンテキストまたは対応する Markdown を使用してください。

error-book-mokuro = 未対応の Mokuro ファイル、無効なローカル画像、またはテキスト座標です。
mokuro-ready = テキストは処理済みです。ページ上のテキスト領域を選択してください。
mokuro-empty-page = このページにはテキスト領域がありません。
mokuro-region = { $page } ページの領域 { $number } を読む

# PHR-001…PHR-003: independent local phrase notebook.
phrases-title = フレーズノート
phrases-new = 新しいフレーズ
phrases-capture = 現在の文章を取り込む
phrases-text = フレーズ
phrases-definition = 意味・定義
phrases-context = 元の文
phrases-language = 学習言語
phrases-save = フレーズを保存
phrases-cancel = キャンセル
phrases-search = フレーズを検索
phrases-export = 表示中のフレーズを書き出す
phrases-edit = フレーズを編集
phrases-delete = フレーズを削除
phrases-source = 出典を開く
phrases-readonly = ライブラリは読み取り専用、または利用できません。
phrases-error = 保存できません。入力の不備、重複、またはノートの上限を確認してください。
phrases-invalid = 文中と同じフレーズを入力してください（文は空欄でも可）。フレーズ・意味は各4 KiB、文は16 KiBまでです。
phrases-source-missing = 元の出典を利用できません。
phrases-status = フレーズの状態

# CLO-001…CLO-004: explicit cloze exercises and opt-in schedules.
cloze-title = 穴埋め練習
cloze-preview = 穴埋めのプレビュー
cloze-create = 問題を作成
cloze-close = 穴埋め練習を閉じる
cloze-scheduled = 予定された復習
cloze-finite = 有限練習・復習予定は変更しません
cloze-correct = 正解
cloze-incorrect = 不正解
cloze-answer = 回答
cloze-submit = 回答を確認
cloze-next = 次の問題
cloze-skip = 問題をスキップ
cloze-complete = 練習終了
cloze-skipped = スキップ済み
cloze-undo = 前の問題を元に戻す
cloze-cancel = 問題一覧に戻る
cloze-instructions = フレーズノートから問題を作成します。表示中の先頭100件まで練習できます。検索で絞り込めます。復習予定への登録は別途行います。
cloze-rules = 回答はNFCと前後の空白除去のみで比較します。大文字・記号・仮名・読み方は区別します。選択式には同じ学習言語の異なる回答が必要です。
cloze-start-typed = 表示中を入力式で練習
cloze-start-choice = 表示中を選択式で練習
cloze-start-due = 期限の来た問題を復習
cloze-enrol = 復習予定に登録
cloze-delete = 問題と復習予定を削除
cloze-error = 操作できません。問題の重複・入力・件数上限、または古い復習予定を確認し、再読み込みしてください。
cloze-unavailable = 対象の問題がないか、文または同じ学習言語の異なる選択肢がありません。
cloze-start-due-choice = 期限の来た問題を選択式で復習

# SEN-001…SEN-003: exact local sentence examples.
bank-title = 例文バンク
bank-close = 例文バンクを閉じる
bank-instructions = 現在の自分の本、読み込み済みの主字幕全体、または所有するJSON例文パックを登録します。辞書の表記と読みで完全一致検索できます。保存済みの例文はオフラインでも利用できます。
bank-index-book = 現在の本を登録
bank-index-track = 主字幕を登録
bank-import = 所有する例文を取り込む
bank-indexing = 例文を準備・保存中…
bank-cancel = 登録を中止
bank-expression = 表記
bank-reading = 読み
bank-search = 完全一致の例文を検索
bank-snapshot = 保存済み例文・出典は開くときに確認します
bank-export = 表示中の例文を書き出す
bank-sources = 登録済みの出典
bank-delete = 保存済みの出典を削除
bank-error = 登録できません。辞書・言語・形式・上限を確認してください。一部のみの保存は行いません。
bank-cancelled = 例文の登録を中止しました。
bank-source-missing = 元の出典を利用できません。保存済みの例文は保持します。
bank-source-changed = 元の文章が変更されています。保存した位置は開きませんでした。
bank-filter = 所有する例文パックJSON
bank-examples = 出典をまたいだ例文

# BMG-001…BMG-004: validated batch exporter/recovery.
batch-invalid = バッチの入力または上限を確認してください。
batch-schema-needed = オフラインのバッチにはLapisを使用するか、このノートタイプのフィールドを明示的に設定してください。
batch-identity-field = バッチの重複確認には単語・読みだけを含む空でない先頭フィールドが必要です。他の設定では単一カードの作成を使用してください。
batch-source-changed = 保存した出典が見つからないか変更されています。取り込みとプレビューをやり直してください。
batch-stale-preview = 設定・辞書・フィールド・出典が変更されています。バッチのプレビューをやり直してください。
batch-reconcile-ambiguous = Anki内の操作を一意に確認できません。カードは再送信しませんでした。
batch-unknown = ノートは既に作成されている可能性があります。同じ操作IDで確認してから対応してください。
batch-title = カードのバッチ作成
batch-close = バッチ画面を閉じる
batch-new = 新しいバッチ
batch-stop = 現在のカードの後で停止
batch-instructions = 選択した主字幕の語彙、検索結果、保存済み例文から追加します。文とメディアを選び、テキストのプレビューを確認してから明示的に開始してください。不明な結果は操作IDで確認します。
batch-details = プレビューを確認
batch-retry = 確定した失敗を再試行
batch-context = 元の文
batch-audio = 文の音声
batch-image = 画像
batch-media-planned = 以下は描画済みのテキストフィールドです。選択したメディアの抽出は開始後に行います。
batch-preview = テキストのプレビューを準備
batch-start = 確認済みのバッチを開始
batch-continue = 残りのカードを続行
batch-history = バッチ履歴
batch-reconcile = Ankiで同じ操作を確認
batch-delete-journal = 解決済みの記録を削除
batch-selected = 選択語をバッチに追加
batch-add-lookup = この語をバッチに追加
batch-add-context = この文をバッチに使用
batch-new-required = 処理中、または開始済みです。終了するか、新しいバッチを作成してから編集してください。
batch-journal-error = 記録を保存できません。上限、重複・未解決の操作、ライブラリの書き込み可否を確認してください。新しい送信は開始しませんでした。
batch-planned = 未送信
batch-sending = 送信中
batch-created = 作成済み
batch-kept = 後で送信するため保存
batch-duplicate = 重複のためスキップ
batch-unresolved = 結果が未解決
batch-failed = 保存前の失敗
batch-cancelled = キャンセル済み

# M108 — Named profiles and explicit composer controls
mining-profiles = カード作成プロファイル
mining-profiles-hint = 保存先、デッキ、ノートタイプ、タグ、フィールドのテンプレートを保存します。変更を保持するには設定を保存してください。
mining-profile-name = プロファイル名
mining-profile-save = 新しいプロファイルを保存
mining-profile-apply = プロファイルを適用：{ $name }
mining-profile-delete = プロファイルを削除：{ $name }
mining-profile-invalid = 無効または重複したプロファイル、もしくは上限の16件を超えています。
composer-fields = フィールドの編集と固定
composer-fields-hint = 有効にしたフィールドはテンプレートの代わりに入力した文字列を使用します。固定は今回の起動中のみ有効で、通常の作成や一括作成には適用されません。
composer-field-value = 入力文字列：{ $name }
composer-pin = フィールドを固定：{ $name }
composer-senses = 辞書の語義を選択
composer-profile-hint = プロファイルを適用すると下書きを閉じ、固定したフィールドをすべて解除します。その後、作成画面を開き直してください。
composer-fields-invalid = フィールドが存在しないか、空、または上限を超えています。ノートタイプと編集内容を確認してください。
composer-senses-stale = 変更されていない語義を少なくとも1つ選択してください。辞書の変更後は作成画面を開き直してください。
composer-source-stale = 元のソースまたは検索が変更されました。下書きを閉じて作成画面を開き直してください。

# M109 — Guided lookup and held secondary peek
settings-pause-lookup = 一時停止時の検索
settings-pause-off = 無効
settings-pause-unknown = 最初の未知語
settings-pause-kanji = 最初の漢字
settings-pause-hint = 最初の表示中の主字幕を使用します。未知語はインストール済み辞書と単語の状態で判定します。漢字の検索は別のルールです。
action-lookup-first-kanji = 最初の漢字を検索
action-peek-translation = 押している間だけ副字幕の訳を表示

# M110 — Additional local documents
error-book-document = この文書は無効、未対応、または構造上の上限を超えています。
error-book-pdf-helper = PDFのテキスト抽出にはPopplerのpdftotextが必要です。詳細設定で実行ファイルを選択するか、対応する環境ではセットアップからインストールしてください。
error-book-document-tool = 文書の抽出に失敗しました：{ $reason }
settings-pdftotext = Poppler pdftotextの実行ファイルまたはフォルダー
setup-tool-system-package = システムパッケージ（管理者認証）
setup-tool-system-package-hint = PopplerはLinuxのパッケージ管理とそのリポジトリ・ネットワーク設定を使用します。インストールには管理者認証が必要な場合があります。停止すると後続の処理を止め、実行中のシステム処理は完了を待ちます。アプリの独自プロキシ設定時やサンドボックス内ではインストールできません。既存のツールは引き続き使用できます。
setup-tool-system-proxy = システムのパッケージ管理でPopplerをインストールするか、既存の実行ファイルを指定してください。システムインストールでアプリのプロキシ設定を回避することはできません。
reader-tools-title = 注釈と検索
reader-tools-close = 閉じる
annotations-title = 注釈
annotations-error = 注釈を保存できませんでした。無効か、重複しているか、上限を超えています。
annotations-no-book = 注釈を付けるにはテキストの本を開いてください。
annotations-export = 表示中を書き出す（TSV）
annotations-search = 注釈を絞り込む
annotations-this-book = この本だけ
annotations-chapter = 章
annotations-stale = 本文が変わりました。この位置は表示しません
annotations-go = 本で開く
annotations-cover = 範囲：
annotations-cover-word = 単語
annotations-cover-sentence = 文
annotations-cover-paragraph = 段落
annotations-quote = 引用した本文
annotations-name = 名前
annotations-note = メモ
annotations-invalid = しおりには名前が必要です。名前に改行は使えません。引用・名前・メモは上限内にしてください。
annotation-kind-bookmark = しおり
annotation-kind-highlight = ハイライト
annotation-kind-note = メモ
annotation-new-bookmark = しおりを追加
annotation-new-highlight = ハイライト
annotation-new-note = メモを追加
search-title = 本を検索
search-instructions = 追加したすべての本の本文から文字列をそのまま検索します（大文字小文字とUnicodeの合成は区別しません）。漫画は検索しません。
search-query = 検索する文字列
search-run = 検索
search-cancel = 中止
search-invalid = 制御文字を含まない1〜256バイトの文字列を入力してください。
search-failed = 検索に失敗したか、中止されました。
search-summary = { $books }冊で{ $hits }件
search-truncated = ほかの結果は省略しました。文字列を絞り込んでください。
search-unreadable = 検索できなかった本（見つからないか変更済み）：{ $books }
books-mode = 読み方
books-mode-scroll = スクロール
books-mode-pages = ページ
books-mode-sentence = 一文ずつ
books-page-of = { $page } / { $pages }ページ
books-sentence-of = { $sentences }文中{ $sentence }文目・{ $paragraph }段落
books-previous-sentence = 前の文
books-next-sentence = 次の文
podcasts-title = ポッドキャスト
podcasts-close = 閉じる
podcasts-instructions = RSSフィードのアドレス（HTTPS）でポッドキャストを追加します。指示しない限り取得もダウンロードもしません。ダウンロードしたエピソードは、文字起こしや自分のタイミング付きテキストと一緒にオフラインで聴けます。
podcasts-url = フィードのアドレス
podcasts-add = 追加または更新
podcasts-fetching = フィードを読み込み中…
podcasts-stop = 中止
podcasts-downloading = ダウンロード中：{ $total } MiB 中 { $done } MiB
podcasts-empty = ポッドキャストはまだありません。
podcasts-refresh = 更新
podcasts-feed-page = ウェブページ
podcasts-remove = ダウンロードごと削除
podcasts-unlisted = （フィードにはもうありません）
podcasts-play = 再生
podcasts-delete = ダウンロードを削除
podcasts-import = タイミング付きテキストを使う…
podcasts-has-text = テキストあり
podcasts-no-text = テキストなし
podcasts-resume = ダウンロードを再開
podcasts-download = ダウンロード
podcasts-episode-page = エピソードのページ
podcasts-error = ポッドキャスト：{ $reason }
podcasts-bad-url = HTTPSのフィードアドレスを入力してください。
podcasts-transcript-unsupported = ダウンロードしました。この文字起こしの形式（{ $kind }）には対応していません。自分のタイミング付きテキストを使えます。
podcasts-transcript-saved = 文字起こしと一緒にダウンロードしました。
podcasts-transcript-none = ダウンロードしました。フィードに文字起こしはありません。自分のタイミング付きテキストを使えます。
podcasts-cancelled = 中止しました。再開すると続きからダウンロードします
podcasts-download-failed = ダウンロードが完了しませんでした：{ $reason }
podcasts-missing-file = ダウンロードしたファイルがありません。もう一度ダウンロードしてください。
shortcut-map-export = ショートカットを書き出す…
shortcut-map-import = ショートカットを読み込む…
shortcut-map-filter = Nikomiru のショートカット設定
shortcut-map-preview = 読み込んだショートカット：確認してから「適用」してください
shortcut-map-no-changes = この設定で変わるものはありません。
shortcut-map-change = { $action }：{ $before } → { $after }
shortcut-map-unknown = このバージョンにない操作は無視します：{ $actions }
shortcut-map-invalid-key = { $action }：「{ $key }」はキーではありません
shortcut-map-apply = 適用
shortcut-map-cancel = キャンセル
shortcut-map-failed = ショートカット設定を読み込めませんでした：{ $reason }
shortcut-map-invalid = Nikomiru のショートカット設定ではありません。
shortcut-map-newer = 新しい Nikomiru で書かれています。
shortcut-map-too-large = 上限（64 KiB、512 操作、各 8 キー）を超えています。
