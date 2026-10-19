// SPDX-License-Identifier: GPL-3.0-only
//
// Spike S1q: a Qt Quick item that shows mpv through the libmpv render API.
// mpv renders into the item's framebuffer object on Qt Quick's render thread;
// the scene graph composites video and QML overlay on the GPU.

#pragma once

#include <QtQuick/QQuickFramebufferObject>
#include <QVariant>

#include <atomic>
#include <memory>
#include <mutex>
#include <vector>

struct mpv_handle;
class MpvItem;

/// Link between the item (GUI thread) and its renderer and mpv callbacks (other
/// threads). The item clears it in its destructor, so late callbacks and a
/// renderer that outlives the item never touch a destroyed object.
struct ItemLink {
    std::mutex mutex;
    MpvItem *item = nullptr;
};

class MpvItem : public QQuickFramebufferObject {
    Q_OBJECT
    Q_PROPERTY(double timePos READ timePos NOTIFY timePosChanged)
    Q_PROPERTY(bool paused READ paused NOTIFY pausedChanged)

public:
    explicit MpvItem(QQuickItem *parent = nullptr);
    ~MpvItem() override;

    Renderer *createRenderer() const override;

    double timePos() const { return m_timePos; }
    bool paused() const { return m_paused; }

    Q_INVOKABLE void load(const QString &path);
    Q_INVOKABLE void command(const QStringList &args);
    Q_INVOKABLE void setOption(const QString &name, const QString &value);
    Q_INVOKABLE QVariant getProperty(const QString &name) const;
    /// Counters for the measurements: renders, new frames, render CPU time.
    Q_INVOKABLE QVariantMap stats() const;
    Q_INVOKABLE void resetRenderTimes();

    // Used by the renderer (render thread).
    std::shared_ptr<mpv_handle> handle() const { return m_mpv; }
    std::shared_ptr<ItemLink> link() const { return m_link; }
    void recordRender(qint64 micros, bool newFrame, QSize size);

signals:
    void timePosChanged();
    void pausedChanged();
    void fileLoaded();

private slots:
    void onMpvEvents();
    /// Called (queued) by the renderer once the render context exists: with
    /// vo=libmpv, a file loaded before that has no video output.
    void onRendererReady();

private:
    std::shared_ptr<ItemLink> m_link = std::make_shared<ItemLink>();
    std::shared_ptr<mpv_handle> m_mpv;
    double m_timePos = 0.0;
    double m_pendingTime = -1.0;
    qint64 m_lastTimeEmit = 0;
    bool m_paused = false;
    bool m_rendererReady = false;
    QString m_pendingPath;

    std::atomic<quint64> m_renders{0};
    std::atomic<quint64> m_newFrames{0};
    mutable std::mutex m_timesMutex;
    std::vector<quint32> m_renderTimes;
    QSize m_fboSize;
};

extern "C" void s1q_register();
