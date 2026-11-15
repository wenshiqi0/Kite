#include "media.h"
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mftransform.h>
#include <mferror.h>
#include <mmsystem.h>
#include <opus.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <condition_variable>
#include <cstring>
#include <mutex>
#include <thread>
#include <vector>

namespace {
constexpr size_t MAX_FRAME = 8 * 1024 * 1024;
constexpr GUID H264_DECODER = {0x62ce7e72, 0x4c71, 0x4d20, {0xb1,0x5d,0x45,0x28,0x31,0xa8,0x7d,0x9d}};
constexpr GUID LOW_LATENCY = {0x9c27891a,0xed7a,0x40e1,{0x88,0xe8,0xb2,0x27,0x27,0xa0,0x24,0xee}};
template<class T> struct Com {
    T* value = nullptr;
    ~Com() { reset(); }
    void reset() { if (value) { value->Release(); value = nullptr; } }
    T** out() { reset(); return &value; }
    T* operator->() const { return value; }
};
struct Frame { std::vector<uint8_t> bytes; uint64_t pts=0, generation=0; uint32_t flags=0; };
struct Audio { std::array<uint8_t,4096> bytes{}; uint32_t length=0; uint64_t generation=0; };
struct Decoder {
    void* context; KitePixels pixels; KiteMediaError error;
    std::mutex mutex, audio_mutex;
    std::condition_variable wake, audio_wake;
    std::atomic<bool> exiting{false}, muted{false};
    std::atomic<uint64_t> generation{0};
    std::array<Frame,6> frames;
    size_t read=0, count=0;
    uint32_t width=0,height=0,fps=0;
    bool configure_pending=false, stop_pending=false;
    std::array<Audio,32> audio_frames;
    size_t audio_read=0,audio_count=0;
    uint32_t rate=0,channels=0,samples=0;
    bool audio_configure_pending=false, audio_stop_pending=false;
    std::thread video_thread, audio_thread;
    Decoder(void* c,KitePixels p,KiteMediaError e):context(c),pixels(p),error(e) {
        // Capacity is reserved once. The transport only copies into bounded slots.
        for (auto& frame : frames) frame.bytes.reserve(MAX_FRAME);
        video_thread=std::thread([this]{video();});
        audio_thread=std::thread([this]{audio();});
    }
    ~Decoder() {
        exiting=true; wake.notify_all(); audio_wake.notify_all();
        video_thread.join(); audio_thread.join();
    }
    void fail(HRESULT hr,uint64_t gen) { if (gen==generation.load()) error(context,static_cast<int32_t>(hr),gen); }
    static HRESULT output_type(IMFTransform* transform,uint32_t& w,uint32_t& h,int32_t& stride) {
        for (DWORD index=0;;++index) {
            Com<IMFMediaType> type;
            HRESULT hr=transform->GetOutputAvailableType(0,index,type.out());
            if (FAILED(hr)) return hr;
            GUID subtype{};
            if (SUCCEEDED(type->GetGUID(MF_MT_SUBTYPE,&subtype)) && subtype==MFVideoFormat_NV12) {
                hr=transform->SetOutputType(0,type.value,0);
                if (FAILED(hr)) return hr;
                UINT32 width=0,height=0,s=0;
                hr=MFGetAttributeSize(type.value,MF_MT_FRAME_SIZE,&width,&height);
                if (FAILED(hr) || !width || !height || width>8192 || height>4320 || static_cast<uint64_t>(width)*height>16777216) return E_INVALIDARG;
                w=width;h=height;
                stride=SUCCEEDED(type->GetUINT32(MF_MT_DEFAULT_STRIDE,&s)) ? static_cast<int32_t>(s) : static_cast<int32_t>(w);
                return stride>=static_cast<int32_t>(w) ? S_OK : E_INVALIDARG;
            }
        }
    }
    HRESULT drain(IMFTransform* transform,uint32_t& w,uint32_t& h,int32_t& stride,
                  uint64_t gen,std::vector<uint8_t>& bgra) {
        for (;;) {
            MFT_OUTPUT_STREAM_INFO info{};
            HRESULT hr=transform->GetOutputStreamInfo(0,&info); if (FAILED(hr)) return hr;
            Com<IMFSample> sample; Com<IMFMediaBuffer> buffer;
            MFT_OUTPUT_DATA_BUFFER out{}; out.dwStreamID=0;
            if (!(info.dwFlags&MFT_OUTPUT_STREAM_PROVIDES_SAMPLES)) {
                hr=MFCreateSample(sample.out()); if (FAILED(hr)) return hr;
                hr=MFCreateMemoryBuffer(std::max<DWORD>(info.cbSize,static_cast<DWORD>(stride*h*3/2)),buffer.out()); if (FAILED(hr)) return hr;
                hr=sample->AddBuffer(buffer.value); if (FAILED(hr)) return hr;
                out.pSample=sample.value;
            }
            DWORD status=0;
            hr=transform->ProcessOutput(0,1,&out,&status);
            if (out.pEvents) out.pEvents->Release();
            Com<IMFSample> owned;
            if (out.pSample && out.pSample!=sample.value) owned.value=out.pSample;
            if (hr==MF_E_TRANSFORM_NEED_MORE_INPUT) return S_OK;
            if (hr==MF_E_TRANSFORM_STREAM_CHANGE) {
                hr=output_type(transform,w,h,stride); if (FAILED(hr)) return hr;
                continue;
            }
            if (FAILED(hr)) return hr;
            if (!out.pSample) continue;
            Com<IMFMediaBuffer> image;
            hr=out.pSample->ConvertToContiguousBuffer(image.out()); if (FAILED(hr)) return hr;
            BYTE* data=nullptr; DWORD length=0;
            hr=image->Lock(&data,nullptr,&length); if (FAILED(hr)) return hr;
            if (length < static_cast<uint64_t>(stride)*h*3/2) { image->Unlock(); return E_INVALIDARG; }
            bgra.resize(static_cast<size_t>(w)*h*4);
            // Sunshine sends limited-range Rec.709. NV12 padding never becomes visible pixels.
            for (uint32_t y=0;y<h;++y) {
                const auto* yy=data+static_cast<size_t>(y)*stride;
                const auto* uv=data+static_cast<size_t>(h+y/2)*stride;
                auto* dst=bgra.data()+static_cast<size_t>(y)*w*4;
                for (uint32_t x=0;x<w;++x) {
                    const int c=std::max(0,int(yy[x])-16),d=int(uv[x&~1u])-128,e=int(uv[(x&~1u)+1])-128;
                    dst[x*4]=static_cast<uint8_t>(std::clamp((298*c+541*d+128)>>8,0,255));
                    dst[x*4+1]=static_cast<uint8_t>(std::clamp((298*c-55*d-136*e+128)>>8,0,255));
                    dst[x*4+2]=static_cast<uint8_t>(std::clamp((298*c+459*e+128)>>8,0,255)); dst[x*4+3]=255;
                }
            }
            image->Unlock();
            if (gen==generation.load()) pixels(context,bgra.data(),w,h,w*4,gen);
        }
    }
    void video() {
        HRESULT initialized=CoInitializeEx(nullptr,COINIT_MULTITHREADED);
        if (FAILED(initialized)) { fail(initialized,generation.load()); return; }
        HRESULT started=MFStartup(MF_VERSION,MFSTARTUP_LITE);
        if (FAILED(started)) { fail(started,generation.load());CoUninitialize();return; }
        {
            Com<IMFTransform> transform;
            std::vector<uint8_t> bgra,config;
            Frame frame; frame.bytes.reserve(MAX_FRAME);
            uint32_t w=0,h=0,f=0; int32_t stride=0; uint64_t gen=0;
            while (!exiting) {
                bool configure=false,stop=false,has_frame=false;
                {
                    std::unique_lock lock(mutex);
                    wake.wait(lock,[this]{return exiting||configure_pending||stop_pending||count;});
                    if (exiting) break;
                    stop=stop_pending;stop_pending=false;
                    configure=configure_pending;configure_pending=false;
                    if (configure) { w=width;h=height;f=fps;gen=generation.load(); }
                    if (count) { std::swap(frame,frames[read]); read=(read+1)%frames.size();--count;has_frame=true; }
                }
                if (stop) {transform.reset();config.clear();}
                if (configure) {
                    transform.reset(); config.clear(); stride=static_cast<int32_t>(w);
                    HRESULT hr=CoCreateInstance(H264_DECODER,nullptr,CLSCTX_INPROC_SERVER,IID_IMFTransform,reinterpret_cast<void**>(transform.out()));
                    Com<IMFMediaType> type;
                    if (SUCCEEDED(hr)) hr=MFCreateMediaType(type.out());
                    if (SUCCEEDED(hr)) hr=type->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video);
                    if (SUCCEEDED(hr)) hr=type->SetGUID(MF_MT_SUBTYPE,MFVideoFormat_H264);
                    if (SUCCEEDED(hr)) hr=MFSetAttributeSize(type.value,MF_MT_FRAME_SIZE,w,h);
                    if (SUCCEEDED(hr)) hr=MFSetAttributeRatio(type.value,MF_MT_FRAME_RATE,f,1);
                    if (SUCCEEDED(hr)) hr=type->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive);
                    if (SUCCEEDED(hr)) hr=transform->SetInputType(0,type.value,0);
                    if (SUCCEEDED(hr)) hr=output_type(transform.value,w,h,stride);
                    if (SUCCEEDED(hr)) {
                        Com<IMFAttributes> attributes;
                        if (SUCCEEDED(transform->GetAttributes(attributes.out()))) attributes->SetUINT32(LOW_LATENCY,1);
                        hr=transform->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,0);
                        if (SUCCEEDED(hr)) hr=transform->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM,0);
                    }
                    if (FAILED(hr)) {fail(hr,gen);transform.reset();}
                }
                if (!has_frame || !transform.value || frame.generation!=gen || gen!=generation.load()) continue;
                if (frame.flags&1) { config.assign(frame.bytes.begin(),frame.bytes.end()); continue; }
                Com<IMFSample> sample;Com<IMFMediaBuffer> buffer;
                size_t length=config.size()+frame.bytes.size();
                if (length>MAX_FRAME) {fail(E_INVALIDARG,gen);continue;}
                HRESULT hr=MFCreateSample(sample.out());
                if (SUCCEEDED(hr)) hr=MFCreateMemoryBuffer(static_cast<DWORD>(length),buffer.out());
                BYTE* data=nullptr;
                if (SUCCEEDED(hr)) hr=buffer->Lock(&data,nullptr,nullptr);
                if (SUCCEEDED(hr)) {
                    if (!config.empty()) std::memcpy(data,config.data(),config.size());
                    std::memcpy(data+config.size(),frame.bytes.data(),frame.bytes.size());
                    buffer->Unlock(); buffer->SetCurrentLength(static_cast<DWORD>(length));config.clear();
                    hr=sample->AddBuffer(buffer.value);
                }
                if (SUCCEEDED(hr)) hr=sample->SetSampleTime(frame.pts*10);
                if (SUCCEEDED(hr)) hr=sample->SetSampleDuration(10000000/std::max(1u,f));
                if (SUCCEEDED(hr) && (frame.flags&2)) hr=sample->SetUINT32(MFSampleExtension_CleanPoint,1);
                if (SUCCEEDED(hr)) {
                    hr=transform->ProcessInput(0,sample.value,0);
                    if (hr==MF_E_NOTACCEPTING) {
                        hr=drain(transform.value,w,h,stride,gen,bgra);
                        if (SUCCEEDED(hr)) hr=transform->ProcessInput(0,sample.value,0);
                    }
                }
                if (SUCCEEDED(hr)) hr=drain(transform.value,w,h,stride,gen,bgra);
                if (FAILED(hr)) fail(hr,gen);
            }
        }
        MFShutdown();CoUninitialize();
    }
    void audio() {
        HWAVEOUT output=nullptr; OpusDecoder* opus=nullptr;
        HANDLE event=CreateEventW(nullptr,FALSE,FALSE,nullptr);
        std::array<WAVEHDR,8> headers{};
        std::array<std::array<int16_t,5760*2>,8> pcm{};
        size_t slot=0;uint64_t gen=0; uint32_t sample_rate=0;
        const auto close=[&] {
            if (output) { waveOutReset(output); for(auto& header:headers) if(header.dwFlags&WHDR_PREPARED) waveOutUnprepareHeader(output,&header,sizeof(header));waveOutClose(output);output=nullptr; }
            headers={}; if(opus){opus_decoder_destroy(opus);opus=nullptr;}
        };
        while (!exiting) {
            Audio packet; bool configure=false,stop=false,has_packet=false;uint32_t r=0,c=0;
            {
                std::unique_lock lock(audio_mutex);
                audio_wake.wait(lock,[this]{return exiting||audio_configure_pending||audio_stop_pending||audio_count;});
                if(exiting) break;
                configure=audio_configure_pending;audio_configure_pending=false;
                stop=audio_stop_pending;audio_stop_pending=false;
                if(configure){r=rate;c=channels;gen=generation.load();}
                if(audio_count){packet=audio_frames[audio_read];audio_read=(audio_read+1)%audio_frames.size();--audio_count;has_packet=true;}
            }
            if(stop)close();
            if(configure){
                close();int error_code=0;sample_rate=r;
                opus=opus_decoder_create(r,c,&error_code);
                WAVEFORMATEX format{};format.wFormatTag=WAVE_FORMAT_PCM;format.nChannels=2;format.nSamplesPerSec=r;format.wBitsPerSample=16;format.nBlockAlign=4;format.nAvgBytesPerSec=r*4;
                if(!opus||error_code!=OPUS_OK||waveOutOpen(&output,WAVE_MAPPER,&format,reinterpret_cast<DWORD_PTR>(event),0,CALLBACK_EVENT)!=MMSYSERR_NOERROR){close();continue;}
            }
            if(!has_packet||!opus||!output||packet.generation!=gen||gen!=generation.load())continue;
            auto& header=headers[slot];
            // Keep playback bounded; skip sound during device stalls rather than blocking video.
            if((header.dwFlags&WHDR_PREPARED)&&!(header.dwFlags&WHDR_DONE)) {
                WaitForSingleObject(event,20);
                if(!(header.dwFlags&WHDR_DONE))continue;
            }
            if(header.dwFlags&WHDR_PREPARED)waveOutUnprepareHeader(output,&header,sizeof(header));
            int n=opus_decode(opus,packet.bytes.data(),packet.length,pcm[slot].data(),5760,0);
            if(n<=0)continue;
            if(muted)std::fill_n(pcm[slot].data(),n*2,0);
            header={};header.lpData=reinterpret_cast<LPSTR>(pcm[slot].data());header.dwBufferLength=n*4;
            if(waveOutPrepareHeader(output,&header,sizeof(header))==MMSYSERR_NOERROR)waveOutWrite(output,&header,sizeof(header));
            slot=(slot+1)%headers.size();(void)sample_rate;
        }
        close();CloseHandle(event);
    }
};
}

extern "C" {
void* kite_decoder_create(void* context,KitePixels pixels,KiteMediaError error) {
    if(!context||!pixels||!error)return nullptr;
    try{return new Decoder(context,pixels,error);}catch(...){return nullptr;}
}
void kite_decoder_destroy(void* p){delete static_cast<Decoder*>(p);}
uint8_t kite_decoder_configure(void* p,uint32_t w,uint32_t h,uint32_t fps,uint64_t gen){
    if(!p||!w||!h||w>8192||h>8192||w%2||h%2||!fps||fps>120)return 0;
    auto& d=*static_cast<Decoder*>(p);
    std::lock_guard lock(d.mutex);d.generation=gen;d.width=w;d.height=h;d.fps=fps;d.read=d.count=0;d.configure_pending=true;d.wake.notify_one();return 1;
}
uint8_t kite_decoder_frame(void* p,const uint8_t* data,uint32_t length,uint64_t pts,uint32_t flags,uint64_t gen){
    if(!p||!data||!length||length>MAX_FRAME)return 0;
    auto& d=*static_cast<Decoder*>(p);std::unique_lock lock(d.mutex,std::try_to_lock);
    if(!lock||gen!=d.generation||d.count==d.frames.size())return 0;
    auto& f=d.frames[(d.read+d.count)%d.frames.size()];f.bytes.assign(data,data+length);f.pts=pts;f.flags=flags;f.generation=gen;
    ++d.count;d.wake.notify_one();return 1;
}
void kite_decoder_audio_configure(void* p,uint32_t rate,uint32_t channels,uint32_t samples,uint64_t gen){
    if(!p||channels!=2||rate!=48000||!samples||samples>5760)return;
    auto& d=*static_cast<Decoder*>(p);std::lock_guard lock(d.audio_mutex);
    if(gen!=d.generation)return;
    d.rate=rate;d.channels=channels;d.samples=samples;d.audio_read=d.audio_count=0;d.audio_configure_pending=true;d.audio_wake.notify_one();
}
void kite_decoder_audio(void* p,const uint8_t* data,uint32_t length,uint64_t gen){
    if(!p||!data||!length||length>4096)return;
    auto& d=*static_cast<Decoder*>(p);std::unique_lock lock(d.audio_mutex,std::try_to_lock);
    if(!lock||gen!=d.generation)return;
    if(d.audio_count==d.audio_frames.size()){d.audio_read=(d.audio_read+1)%d.audio_frames.size();--d.audio_count;}
    auto& f=d.audio_frames[(d.audio_read+d.audio_count)%d.audio_frames.size()];std::memcpy(f.bytes.data(),data,length);f.length=length;f.generation=gen;
    ++d.audio_count;d.audio_wake.notify_one();
}
void kite_decoder_mute(void* p,uint8_t mute){if(p)static_cast<Decoder*>(p)->muted=mute!=0;}
void kite_decoder_stop(void* p,uint64_t gen){
    if(!p)return;auto& d=*static_cast<Decoder*>(p);d.generation=gen;
    {std::lock_guard lock(d.mutex);d.count=0;d.configure_pending=false;d.stop_pending=true;}d.wake.notify_one();
    {std::lock_guard lock(d.audio_mutex);d.audio_count=0;d.audio_configure_pending=false;d.audio_stop_pending=true;}d.audio_wake.notify_one();
}
}
