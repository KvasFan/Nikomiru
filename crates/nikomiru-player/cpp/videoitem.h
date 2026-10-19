// SPDX-License-Identifier: GPL-3.0-only
//
// The Qt Quick video item (SPECIFICATION §5.2, ADR 0003). The mpv core belongs
// to Rust (nikomiru-player's Player); this item only renders it, with the libmpv
// render API, on Qt Quick's render thread.

#pragma once

#include <QtQuick/QQuickFramebufferObject>

#include <memory>
#include <mutex>

class VideoItem;

/// Link between the item (GUI thread) and its renderer and mpv's update callback
/// (other threads). The item clears it in its destructor, so a late callback or a
/// renderer that outlives the item never touches a destroyed object (§5.2.2).
struct VideoItemLink {
    std::mutex mutex;
    VideoItem *item = nullptr;
};

class VideoItem : public QQuickFramebufferObject {
    Q_OBJECT
    /// True once the render context exists: only then may a file be loaded,
    /// because with vo=libmpv a file loaded earlier has no video output.
    Q_PROPERTY(bool rendererReady READ rendererReady NOTIFY rendererReadyChanged)

public:
    explicit VideoItem(QQuickItem *parent = nullptr);
    ~VideoItem() override;

    Renderer *createRenderer() const override;
    bool rendererReady() const { return m_ready; }
    std::shared_ptr<VideoItemLink> link() const { return m_link; }

signals:
    void rendererReadyChanged();

private slots:
    void onRendererReady();

private:
    std::shared_ptr<VideoItemLink> m_link = std::make_shared<VideoItemLink>();
    bool m_ready = false;
};

/// Selects the OpenGL scene graph and registers `VideoItem` in the QML module
/// `Nikomiru.Video`. Call after QGuiApplication and before the first window.
void nikomiru_video_init();

/// The mpv core to render (an `mpv_handle *`, owned by Rust). Call before the
/// QML engine creates the item; the core must outlive every item.
void nikomiru_video_set_core(std::size_t core);
