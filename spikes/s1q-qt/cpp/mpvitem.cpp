// SPDX-License-Identifier: GPL-3.0-only
//
// Spike S1q: see mpvitem.h. Pattern of mpv's official Qt Quick example and of
// KDE's MpvQt/Haruna, written from scratch for the measurement.

#include "mpvitem.h"

#include <mpv/client.h>
#include <mpv/render_gl.h>

#include <QDateTime>
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QOpenGLContext>
#include <QOpenGLFramebufferObject>
#include <QQuickWindow>
#include <QtQml/qqml.h>

#include <algorithm>
#include <clocale>

namespace {

void *getProcAddress(void *, const char *name) {
    QOpenGLContext *ctx = QOpenGLContext::currentContext();
    return ctx ? reinterpret_cast<void *>(ctx->getProcAddress(QByteArray(name))) : nullptr;
}

void onRenderUpdate(void *ctx) {
    // Called from any mpv thread: never call mpv here, only schedule a repaint.
    auto *link = static_cast<ItemLink *>(ctx);
    std::lock_guard lock(link->mutex);
    if (link->item) {
        QMetaObject::invokeMethod(link->item, "update", Qt::QueuedConnection);
    }
}

void onWakeup(void *ctx) {
    QMetaObject::invokeMethod(static_cast<MpvItem *>(ctx), "onMpvEvents", Qt::QueuedConnection);
}

class MpvRenderer : public QQuickFramebufferObject::Renderer {
public:
    explicit MpvRenderer(MpvItem *item) : m_link(item->link()), m_mpv(item->handle()) {}

    ~MpvRenderer() override {
        // The render context goes before the mpv core (shared_ptr keeps the core alive).
        if (m_ctx) {
            mpv_render_context_free(m_ctx);
        }
    }

    QOpenGLFramebufferObject *createFramebufferObject(const QSize &size) override {
        if (!m_ctx) {
            mpv_opengl_init_params gl{getProcAddress, nullptr};
            int advanced = 0;
            std::vector<mpv_render_param> params{
                {MPV_RENDER_PARAM_API_TYPE, const_cast<char *>(MPV_RENDER_API_TYPE_OPENGL)},
                {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &gl},
                {MPV_RENDER_PARAM_ADVANCED_CONTROL, &advanced},
            };
            // Native display for zero-copy hwdec (VA-API over EGL).
            if (auto *wl = qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>()) {
                params.push_back({MPV_RENDER_PARAM_WL_DISPLAY, wl->display()});
            } else if (auto *x11 = qGuiApp->nativeInterface<QNativeInterface::QX11Application>()) {
                params.push_back({MPV_RENDER_PARAM_X11_DISPLAY, x11->display()});
            }
            params.push_back({MPV_RENDER_PARAM_INVALID, nullptr});
            if (mpv_render_context_create(&m_ctx, m_mpv.get(), params.data()) < 0) {
                qWarning("[s1q] mpv_render_context_create failed");
                m_ctx = nullptr;
            } else {
                mpv_render_context_set_update_callback(m_ctx, onRenderUpdate, m_link.get());
                std::lock_guard lock(m_link->mutex);
                if (m_link->item) {
                    QMetaObject::invokeMethod(m_link->item, "onRendererReady", Qt::QueuedConnection);
                }
            }
        }
        return QQuickFramebufferObject::Renderer::createFramebufferObject(size);
    }

    void render() override {
        if (!m_ctx) {
            return;
        }
        QElapsedTimer t;
        t.start();
        const bool newFrame = mpv_render_context_update(m_ctx) & MPV_RENDER_UPDATE_FRAME;
        QOpenGLFramebufferObject *fbo = framebufferObject();
        mpv_opengl_fbo target{static_cast<int>(fbo->handle()), fbo->width(), fbo->height(), 0};
        int flip = 0;
        mpv_render_param params[] = {
            {MPV_RENDER_PARAM_OPENGL_FBO, &target},
            {MPV_RENDER_PARAM_FLIP_Y, &flip},
            {MPV_RENDER_PARAM_INVALID, nullptr},
        };
        mpv_render_context_render(m_ctx, params);
        {
            std::lock_guard lock(m_link->mutex);
            if (m_link->item) {
                m_link->item->recordRender(t.nsecsElapsed() / 1000, newFrame, fbo->size());
            }
        }
        static int debugCount = 0;
        if (qEnvironmentVariableIsSet("S1Q_DEBUG") && debugCount++ < 5) {
            qWarning("[s1q] render %dx%d new=%d", fbo->width(), fbo->height(), int(newFrame));
        }
    }

private:
    std::shared_ptr<ItemLink> m_link;
    std::shared_ptr<mpv_handle> m_mpv;
    mpv_render_context *m_ctx = nullptr;
};

} // namespace

MpvItem::MpvItem(QQuickItem *parent) : QQuickFramebufferObject(parent) {
    m_link->item = this;
    // libmpv refuses to start unless LC_NUMERIC is "C"; QGuiApplication set it from the environment.
    std::setlocale(LC_NUMERIC, "C");
    mpv_handle *h = mpv_create();
    if (!h) {
        qFatal("[s1q] mpv_create failed");
    }
    // PLY-005: fixed options before initialize.
    const char *fixed[][2] = {
        {"vo", "libmpv"},       {"config", "no"},     {"input-default-bindings", "no"},
        {"input-vo-keyboard", "no"}, {"osc", "no"},  {"terminal", "no"},
        {"idle", "yes"},        {"keep-open", "yes"}, {"ytdl", "no"},
        {"sub-auto", "no"},     {"hwdec", "auto-safe"},
    };
    for (auto &kv : fixed) {
        mpv_set_option_string(h, kv[0], kv[1]);
    }
    const QByteArray extra = qgetenv("S1Q_MPV_OPTIONS"); // "key=value,key=value"
    for (const QByteArray &kv : extra.split(',')) {
        const int eq = kv.indexOf('=');
        if (eq > 0) {
            mpv_set_option_string(h, kv.left(eq).constData(), kv.mid(eq + 1).constData());
        }
    }
    if (mpv_initialize(h) < 0) {
        qFatal("[s1q] mpv_initialize failed");
    }
    m_mpv = std::shared_ptr<mpv_handle>(h, [](mpv_handle *p) { mpv_terminate_destroy(p); });
    mpv_observe_property(h, 0, "time-pos", MPV_FORMAT_DOUBLE);
    mpv_observe_property(h, 0, "pause", MPV_FORMAT_FLAG);
    mpv_set_wakeup_callback(h, onWakeup, this);
}

MpvItem::~MpvItem() {
    {
        std::lock_guard lock(m_link->mutex);
        m_link->item = nullptr;
    }
    if (m_mpv) {
        mpv_set_wakeup_callback(m_mpv.get(), nullptr, nullptr);
    }
}

QQuickFramebufferObject::Renderer *MpvItem::createRenderer() const {
    window()->setPersistentGraphics(true);
    window()->setPersistentSceneGraph(true);
    return new MpvRenderer(const_cast<MpvItem *>(this));
}

void MpvItem::onRendererReady() {
    m_rendererReady = true;
    if (!m_pendingPath.isEmpty()) {
        load(m_pendingPath);
        m_pendingPath.clear();
    }
}

void MpvItem::load(const QString &path) {
    if (!m_rendererReady) {
        m_pendingPath = path;
        return;
    }
    const QByteArray p = path.toUtf8();
    const char *args[] = {"loadfile", p.constData(), "replace", nullptr};
    mpv_command_async(m_mpv.get(), 0, args);
}

void MpvItem::command(const QStringList &args) {
    std::vector<QByteArray> storage;
    std::vector<const char *> argv;
    for (const QString &a : args) {
        storage.push_back(a.toUtf8());
    }
    for (const QByteArray &a : storage) {
        argv.push_back(a.constData());
    }
    argv.push_back(nullptr);
    mpv_command_async(m_mpv.get(), 0, argv.data());
}

void MpvItem::setOption(const QString &name, const QString &value) {
    mpv_set_property_string(m_mpv.get(), name.toUtf8().constData(), value.toUtf8().constData());
}

QVariant MpvItem::getProperty(const QString &name) const {
    mpv_node node;
    const int err = mpv_get_property(m_mpv.get(), name.toUtf8().constData(), MPV_FORMAT_NODE, &node);
    if (err < 0) {
        if (qEnvironmentVariableIsSet("S1Q_DEBUG")) {
            qWarning("[s1q] get_property %s: %s", name.toUtf8().constData(), mpv_error_string(err));
        }
        return {};
    }
    QVariant out;
    switch (node.format) {
    case MPV_FORMAT_STRING: out = QString::fromUtf8(node.u.string); break;
    case MPV_FORMAT_FLAG: out = bool(node.u.flag); break;
    case MPV_FORMAT_INT64: out = qlonglong(node.u.int64); break;
    case MPV_FORMAT_DOUBLE: out = node.u.double_; break;
    default: break;
    }
    mpv_free_node_contents(&node);
    return out;
}

void MpvItem::recordRender(qint64 micros, bool newFrame, QSize size) {
    m_renders++;
    if (newFrame) {
        m_newFrames++;
    }
    std::lock_guard lock(m_timesMutex);
    m_renderTimes.push_back(static_cast<quint32>(micros));
    m_fboSize = size;
}

QVariantMap MpvItem::stats() const {
    std::vector<quint32> times;
    QSize size;
    {
        std::lock_guard lock(m_timesMutex);
        times = m_renderTimes;
        size = m_fboSize;
    }
    std::sort(times.begin(), times.end());
    auto pct = [&](double q) -> quint32 {
        if (times.empty()) return 0;
        size_t i = static_cast<size_t>(std::ceil(times.size() * q));
        return times[std::min(times.size() - 1, i == 0 ? 0 : i - 1)];
    };
    return {
        {"renders", QVariant::fromValue<quint64>(m_renders.load())},
        {"new_frames", QVariant::fromValue<quint64>(m_newFrames.load())},
        {"render_us_p50", pct(0.5)},
        {"render_us_p95", pct(0.95)},
        {"render_us_max", times.empty() ? 0u : times.back()},
        {"fbo_width", size.width()},
        {"fbo_height", size.height()},
        {"device_pixel_ratio", window() ? window()->effectiveDevicePixelRatio() : 0.0},
        {"render_thread", window() ? window()->property("renderLoop") : QVariant()},
    };
}

void MpvItem::resetRenderTimes() {
    std::lock_guard lock(m_timesMutex);
    m_renderTimes.clear();
}

void MpvItem::onMpvEvents() {
    while (m_mpv) {
        mpv_event *ev = mpv_wait_event(m_mpv.get(), 0);
        if (ev->event_id == MPV_EVENT_NONE) {
            break;
        }
        if (ev->event_id == MPV_EVENT_FILE_LOADED) {
            emit fileLoaded();
        } else if (ev->event_id == MPV_EVENT_PROPERTY_CHANGE) {
            auto *p = static_cast<mpv_event_property *>(ev->data);
            if (!p->data) continue;
            if (qstrcmp(p->name, "time-pos") == 0) {
                m_pendingTime = *static_cast<double *>(p->data);
            } else if (qstrcmp(p->name, "pause") == 0) {
                m_paused = *static_cast<int *>(p->data);
                emit pausedChanged();
            }
        }
    }
    // ARC-004: at most 30 time-pos updates per second, always the latest value.
    const qint64 now = QDateTime::currentMSecsSinceEpoch();
    if (m_pendingTime >= 0 && now - m_lastTimeEmit >= 33) {
        m_timePos = m_pendingTime;
        m_pendingTime = -1.0;
        m_lastTimeEmit = now;
        emit timePosChanged();
    }
}

extern "C" void s1q_register() {
    // QQuickFramebufferObject needs the OpenGL backend of the scene graph.
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
    qmlRegisterType<MpvItem>("Anikoto.Video", 1, 0, "MpvItem");
}
