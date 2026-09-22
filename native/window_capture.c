// SPDX-License-Identifier: GPL-3.0-only
// Hyprland surface capture -> fixed-canvas YUV420P NUT stream on stdout.
#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>
#include "toplevel.h"

static struct wl_display *display;
static struct wl_shm *shm;
static struct hyprland_toplevel_export_manager_v1 *manager;
static struct wl_buffer *buffer;
static unsigned char *mapping;
static size_t mapping_size;
static uint32_t width, height, stride, format, flags;
static bool done, failed;
static AVFormatContext *output;
static AVStream *stream;
static AVFrame *canvas;
static struct SwsContext *scaler;
static int out_width, out_height;
static int64_t frame_number;

static void die(const char *reason) { fprintf(stderr, "%s\n", reason); exit(1); }
static void on_global(void *data, struct wl_registry *registry, uint32_t name,
                      const char *interface, uint32_t version) {
    (void)data; (void)version;
    if (!strcmp(interface, "wl_shm"))
        shm = wl_registry_bind(registry, name, &wl_shm_interface, 1);
    if (!strcmp(interface, "hyprland_toplevel_export_manager_v1"))
        manager = wl_registry_bind(registry, name, &hyprland_toplevel_export_manager_v1_interface, 1);
}
static void on_remove(void *d, struct wl_registry *r, uint32_t n) { (void)d; (void)r; (void)n; }
static const struct wl_registry_listener registry_listener = {on_global, on_remove};

static void on_buffer(void *d, struct hyprland_toplevel_export_frame_v1 *f,
                      uint32_t fmt, uint32_t w, uint32_t h, uint32_t s) {
    (void)d; (void)f;
    if (!w || !h || w > 16384 || h > 16384 || s < w * 4 || s > 262144)
        die("Invalid window buffer dimensions");
    if (fmt != WL_SHM_FORMAT_XRGB8888 && fmt != WL_SHM_FORMAT_ARGB8888 &&
        fmt != WL_SHM_FORMAT_XBGR8888 && fmt != WL_SHM_FORMAT_ABGR8888)
        die("Unsupported window pixel format");
    width=w; height=h; stride=s; format=fmt;
}
static void on_flags(void *d, struct hyprland_toplevel_export_frame_v1 *f, uint32_t value) {
    (void)d; (void)f; flags=value;
}
static void on_ready(void *d, struct hyprland_toplevel_export_frame_v1 *f,
                     uint32_t hi, uint32_t lo, uint32_t ns) {
    (void)d; (void)f; (void)hi; (void)lo; (void)ns; done=true;
}
static void on_failed(void *d, struct hyprland_toplevel_export_frame_v1 *f) {
    (void)d; (void)f; done=true; failed=true;
}
static void on_damage(void *d, struct hyprland_toplevel_export_frame_v1 *f,
                      uint32_t x, uint32_t y, uint32_t w, uint32_t h) {
    (void)d; (void)f; (void)x; (void)y; (void)w; (void)h;
}
static void on_dmabuf(void *d, struct hyprland_toplevel_export_frame_v1 *f,
                      uint32_t fmt, uint32_t w, uint32_t h) {
    (void)d; (void)f; (void)fmt; (void)w; (void)h;
}
static void on_buffer_done(void *d, struct hyprland_toplevel_export_frame_v1 *f) {
    (void)d;
    if (!width || !height) die("No shared-memory capture format offered");
    mapping_size=(size_t)stride*height;
    if (mapping_size > INT32_MAX) die("Window buffer too large");
    int fd=memfd_create("omacast-window", MFD_CLOEXEC);
    if (fd<0 || ftruncate(fd, (off_t)mapping_size)<0) die("Cannot allocate window buffer");
    mapping=mmap(NULL, mapping_size, PROT_READ|PROT_WRITE, MAP_SHARED, fd, 0);
    if (mapping==MAP_FAILED) die("Cannot map window buffer");
    struct wl_shm_pool *pool=wl_shm_create_pool(shm, fd, (int)mapping_size);
    buffer=wl_shm_pool_create_buffer(pool, 0, (int)width, (int)height, (int)stride, format);
    wl_shm_pool_destroy(pool); close(fd);
    hyprland_toplevel_export_frame_v1_copy(f, buffer, 1);
}
static const struct hyprland_toplevel_export_frame_v1_listener frame_listener = {
    .buffer=on_buffer, .damage=on_damage, .flags=on_flags, .ready=on_ready,
    .failed=on_failed, .linux_dmabuf=on_dmabuf, .buffer_done=on_buffer_done
};
static void setup_output(void) {
    if (avformat_alloc_output_context2(&output, NULL, "nut", "pipe:1")<0) die("Cannot create NUT output");
    stream=avformat_new_stream(output, NULL);
    if (!stream) die("Cannot create video stream");
    stream->time_base=(AVRational){1,30};
    stream->avg_frame_rate=(AVRational){30,1};
    stream->codecpar->codec_type=AVMEDIA_TYPE_VIDEO;
    stream->codecpar->codec_id=AV_CODEC_ID_RAWVIDEO;
    stream->codecpar->format=AV_PIX_FMT_YUV420P;
    stream->codecpar->codec_tag=avcodec_pix_fmt_to_codec_tag(AV_PIX_FMT_YUV420P);
    stream->codecpar->width=out_width; stream->codecpar->height=out_height;
    stream->codecpar->sample_aspect_ratio=(AVRational){1,1};
    if (avio_open(&output->pb,"pipe:1",AVIO_FLAG_WRITE)<0 || avformat_write_header(output,NULL)<0)
        die("Cannot write NUT header");
    canvas=av_frame_alloc();
    if (!canvas) die("Cannot allocate canvas");
    canvas->format=AV_PIX_FMT_YUV420P; canvas->width=out_width; canvas->height=out_height;
    if (av_frame_get_buffer(canvas,32)<0) die("Cannot allocate canvas pixels");
}
static void write_frame(void) {
    int w=out_width, h=(int)((int64_t)height*out_width/width);
    if (h>out_height) {h=out_height; w=(int)((int64_t)width*out_height/height);}
    w=w/2*2; h=h/2*2;
    if (w<2 || h<2) die("Window aspect ratio is unsupported");
    int left=(out_width-w)/4*2, top=(out_height-h)/4*2;
    for (int p=0;p<3;p++) for(int y=0;y<(p?out_height/2:out_height);y++)
        memset(canvas->data[p]+y*canvas->linesize[p],p?128:16,p?out_width/2:out_width);
    enum AVPixelFormat fmt=(format==WL_SHM_FORMAT_XBGR8888 || format==WL_SHM_FORMAT_ABGR8888)
        ? AV_PIX_FMT_RGBA:AV_PIX_FMT_BGRA;
    scaler=sws_getCachedContext(scaler,width,height,fmt,w,h,AV_PIX_FMT_YUV420P,SWS_BILINEAR,NULL,NULL,NULL);
    if (!scaler) die("Cannot scale window");
    const uint8_t *src[4]={mapping,NULL,NULL,NULL};
    int src_stride[4]={(int)stride,0,0,0};
    if (flags&1) { src[0]+=((size_t)height-1)*stride; src_stride[0]=-(int)stride; }
    uint8_t *dst[4]={canvas->data[0]+top*canvas->linesize[0]+left,
        canvas->data[1]+(top/2)*canvas->linesize[1]+left/2,
        canvas->data[2]+(top/2)*canvas->linesize[2]+left/2,NULL};
    sws_scale(scaler,src,src_stride,0,height,dst,canvas->linesize);
    AVPacket *packet=av_packet_alloc();
    if (!packet || av_new_packet(packet,av_image_get_buffer_size(AV_PIX_FMT_YUV420P,out_width,out_height,1))<0)
        die("Cannot allocate output packet");
    av_image_copy_to_buffer(packet->data,packet->size,(const uint8_t *const *)canvas->data,
        canvas->linesize,AV_PIX_FMT_YUV420P,out_width,out_height,1);
    packet->pts=packet->dts=av_rescale_q(frame_number++,(AVRational){1,30},stream->time_base);
    packet->duration=av_rescale_q(1,(AVRational){1,30},stream->time_base);
    packet->stream_index=stream->index; packet->flags=AV_PKT_FLAG_KEY;
    if (av_write_frame(output,packet)<0) die("Output pipe closed");
    avio_flush(output->pb); av_packet_free(&packet);
}
int main(int argc, char **argv) {
    if (argc!=4) die("Usage: window-capture WINDOW_ADDRESS WIDTH HEIGHT");
    char *end; errno=0; uint64_t address=strtoull(argv[1],&end,16);
    if (errno || *end || !address) die("Invalid window address");
    out_width=atoi(argv[2]); out_height=atoi(argv[3]);
    if (out_width<2 || out_height<2 || out_width>3840 || out_height>2160 || out_width%2 || out_height%2)
        die("Invalid output dimensions");
    display=wl_display_connect(NULL);
    if (!display) die("Cannot connect to Wayland");
    struct wl_registry *registry=wl_display_get_registry(display);
    wl_registry_add_listener(registry,&registry_listener,NULL);
    if (wl_display_roundtrip(display)<0 || !manager || !shm) die("Hyprland window export unavailable");
    setup_output();
    for (;;) {
        struct timespec start, end_time; clock_gettime(CLOCK_MONOTONIC,&start);
        done=false; failed=false; flags=0; width=height=0;
        struct hyprland_toplevel_export_frame_v1 *frame=
            hyprland_toplevel_export_manager_v1_capture_toplevel(manager,1,(uint32_t)address);
        hyprland_toplevel_export_frame_v1_add_listener(frame,&frame_listener,NULL);
        while(!done) if(wl_display_dispatch(display)<0) die("Wayland capture disconnected");
        if(failed) die("Selected window is unavailable or sharing was revoked");
        write_frame();
        hyprland_toplevel_export_frame_v1_destroy(frame);
        wl_buffer_destroy(buffer); munmap(mapping,mapping_size);
        clock_gettime(CLOCK_MONOTONIC,&end_time);
        int64_t elapsed=(end_time.tv_sec-start.tv_sec)*1000000000LL+end_time.tv_nsec-start.tv_nsec;
        if(elapsed<33333333) {struct timespec wait={0,33333333-elapsed}; nanosleep(&wait,NULL);}
    }
}
