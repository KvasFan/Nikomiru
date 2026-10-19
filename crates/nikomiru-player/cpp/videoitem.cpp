// SPDX-License-Identifier: GPL-3.0-only
//
// See videoitem.h. Pattern of mpv's Qt Quick example and of KDE's MpvQt, adapted
// from spike S1q (ADR 0003).

#include "videoitem.h"

#include <mpv/client.h>
#include <mpv/render_gl.h>

#include <QGuiApplication>
#include <QOpenGLContext>
#include <QOpenGLFramebufferObject>
#include <QQuickWindow>
#include <QtQml/qqml.h>

#include <atomic>
#include <vector>

namespace {

std::atomic<mpv_handle *> g_core{nullptr};

void *getProcAddress(void *, const char *name) {
    QOpenGLContext *ctx = QOpenGLContext::currentContext();
    return ctx ? reinterpret_cast<void *>(ctx->getProcAddress(QByteArray(name))) : nullptr;
}

void onRenderUpdate(void *ctx) {
    // Called from any mpv thread: never call mpv here, only schedule a repaint.
    auto *link = static_cast<VideoItemLink *>(ctx);
    std::lock_guard lock(link->mutex);
    if (link->item) {
        QMetaObject::invokeMethod(link->item, "update", Qt::QueuedConnection);
    }
}

class VideoRenderer : public QQuickFramebufferObject::Renderer {
public:
    explicit VideoRenderer(std::shared_ptr<VideoItemLink> link) : m_link(std::move(link)) {}

    ~VideoRenderer() override {
        // First in the shutdown order of §5.2.2: the render context, on the render
        // thread, with the GL context current.
        if (m_ctx) {
            mpv_render_context_free(m_ctx);
        }
    }

    QOpenGLFramebufferObject *createFramebufferObject(const QSize &size) override {
        if (!m_ctx) {
            create();
        }
        return QQuickFramebufferObject::Renderer::createFramebufferObject(size);
    }

    void render() override {
        if (!m_ctx) {
            return;
        }
        QOpenGLFramebufferObject *fbo = framebufferObject();
        mpv_opengl_fbo target{static_cast<int>(fbo->handle()), fbo->width(), fbo->height(), 0};
        int flip = 0;
        mpv_render_param params[] = {
            {MPV_RENDER_PARAM_OPENGL_FBO, &target},
            {MPV_RENDER_PARAM_FLIP_Y, &flip},
            {MPV_RENDER_PARAM_INVALID, nullptr},
        };
        mpv_render_context_render(m_ctx, params);
    }

private:
    void create() {
        mpv_handle *core = g_core.load();
        if (!core) {
            qWarning("nikomiru: no mpv core for the video item");
            return;
        }
        mpv_opengl_init_params gl{getProcAddress, nullptr};
        int advanced = 0;
        std::vector<mpv_render_param> params{
            {MPV_RENDER_PARAM_API_TYPE, const_cast<char *>(MPV_RENDER_API_TYPE_OPENGL)},
            {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &gl},
            {MPV_RENDER_PARAM_ADVANCED_CONTROL, &advanced},
        };
        // Native display for zero-copy hwdec (VA-API over EGL), §5.2.2.
        if (auto *wl = qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>()) {
            params.push_back({MPV_RENDER_PARAM_WL_DISPLAY, wl->display()});
        } else if (auto *x11 = qGuiApp->nativeInterface<QNativeInterface::QX11Application>()) {
            params.push_back({MPV_RENDER_PARAM_X11_DISPLAY, x11->display()});
        }
        params.push_back({MPV_RENDER_PARAM_INVALID, nullptr});
        if (mpv_render_context_create(&m_ctx, core, params.data()) < 0) {
            qWarning("nikomiru: mpv_render_context_create failed");
            m_ctx = nullptr;
            return;
        }
        mpv_render_context_set_update_callback(m_ctx, onRenderUpdate, m_link.get());
        std::lock_guard lock(m_link->mutex);
        if (m_link->item) {
            QMetaObject::invokeMethod(m_link->item, "onRendererReady", Qt::QueuedConnection);
        }
    }

    std::shared_ptr<VideoItemLink> m_link;
    mpv_render_context *m_ctx = nullptr;
};

} // namespace

VideoItem::VideoItem(QQuickItem *parent) : QQuickFramebufferObject(parent) {
    m_link->item = this;
}

VideoItem::~VideoItem() {
    std::lock_guard lock(m_link->mutex);
    m_link->item = nullptr;
}

QQuickFramebufferObject::Renderer *VideoItem::createRenderer() const {
    window()->setPersistentGraphics(true);
    window()->setPersistentSceneGraph(true);
    return new VideoRenderer(m_link);
}

void VideoItem::onRendererReady() {
    if (!m_ready) {
        m_ready = true;
        emit rendererReadyChanged();
    }
}

void nikomiru_video_init() {
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
    qmlRegisterType<VideoItem>("Nikomiru.Video", 1, 0, "VideoItem");
}

void nikomiru_video_set_core(std::size_t core) {
    g_core.store(reinterpret_cast<mpv_handle *>(core));
}
