#define KITE_MEDIA_IMPORT
#include "media.h"
#include <atomic>
#include <chrono>
#include <fstream>
#include <iostream>
#include <iterator>
#include <thread>
#include <vector>
int main(int argc,char** argv){
    if(argc!=2)return 2;
    std::ifstream input(argv[1],std::ios::binary);
    std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(input)),{});
    if(bytes.empty())return 3;
    struct State{std::atomic<int> frames{0},error{0},width{0},height{0};} state;
    auto* decoder=kite_decoder_create(&state,[](void* c,const uint8_t* p,uint32_t w,uint32_t h,uint32_t stride,uint64_t gen){
        auto& state=*static_cast<State*>(c);state.width=w;state.height=h;
        if(p&&w==320&&h==180&&stride==w*4&&gen==1)state.frames++;
    },[](void* c,int32_t error,uint64_t){static_cast<State*>(c)->error=error;});
    if(!decoder)return 4;
    if(!kite_decoder_configure(decoder,320,180,30,1))return 5;
    if(kite_decoder_frame(decoder,bytes.data(),static_cast<uint32_t>(bytes.size()),0,2,99))return 6;
    for(int n=0;n<3;++n){kite_decoder_frame(decoder,bytes.data(),static_cast<uint32_t>(bytes.size()),n*33333,2,1);std::this_thread::sleep_for(std::chrono::milliseconds(100));}
    for(int n=0;n<50&&!state.frames&&!state.error;++n)std::this_thread::sleep_for(std::chrono::milliseconds(100));
    kite_decoder_stop(decoder,2);kite_decoder_destroy(decoder);
    std::cout<<"decoded="<<state.frames<<" size="<<state.width<<"x"<<state.height<<" error="<<state.error<<"\n";
    return state.frames>0&&state.error==0?0:7;
}
