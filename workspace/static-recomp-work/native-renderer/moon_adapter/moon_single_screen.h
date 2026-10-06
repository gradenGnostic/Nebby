#pragma once
#include "fast/renderer3ds/screen_composition.h"
#include "moon_battle_root.h"
#include <SDL2/SDL.h>
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>

namespace MoonSingleScreen {
enum Mode:uint32_t { DualScreen=0,NormalSingleScreen=1,BottomPanel=2,BattleOverlay=3,NativePage=4,BattleRoot=5 };
inline bool Enabled(){static bool enabled=[](){const char*v=std::getenv("NEBBY_MOD_SINGLE_SCREEN");return v&&std::strcmp(v,"1")==0;}();return enabled;}
inline std::atomic<uint32_t> Context{BottomPanel};
inline std::atomic<bool> DebugDual{false},ManualPanel{false};
inline std::atomic<uint32_t> Held{0};
// Retain short host presses even when down/up happen between guest frames.
inline std::atomic<uint32_t> PendingButtons{0},PendingCircle{0};
inline std::atomic<uint64_t> SamplePressEdges{0};
inline std::atomic<int> MouseX{0},MouseY{0};
inline std::atomic<bool> MouseDown{false};
inline std::atomic<bool> BattleRootActive{false};
inline std::array<std::atomic<int>,4> BattleCommands{{-1,-1,-1,-1}};
inline std::atomic<int> BattleClick{-1};
inline std::atomic<uint32_t> WindowWidth{1280},WindowHeight{720},WindowId{0};
inline Mode Current(){if(!Enabled()||DebugDual)return DualScreen;if(ManualPanel)return BottomPanel;return static_cast<Mode>(Context.load());}
inline Fast::Renderer3ds::ScreenRegion BottomRegion(){
 const auto mode=Current();
 if(mode==DualScreen)return {0,.5f,1,.5f};
 // Menus are modal overlays; battle controls retain their separate policy.
 if(mode==BottomPanel)return {.2f,.1f,.6f,.8f};
 return {.59f,.48f,.39f,.50f};
}
struct KeyboardMap {
 SDL_Scancode keys[16]={SDL_SCANCODE_X,SDL_SCANCODE_Z,SDL_SCANCODE_BACKSPACE,SDL_SCANCODE_RETURN,SDL_SCANCODE_RIGHT,SDL_SCANCODE_LEFT,SDL_SCANCODE_UP,SDL_SCANCODE_DOWN,SDL_SCANCODE_E,SDL_SCANCODE_Q,SDL_SCANCODE_S,SDL_SCANCODE_A,SDL_SCANCODE_I,SDL_SCANCODE_K,SDL_SCANCODE_J,SDL_SCANCODE_L};
 KeyboardMap(){
  const char* names[16]={"a","b","select","start","dpad_right","dpad_left","dpad_up","dpad_down","r","l","x","y","circle_up","circle_down","circle_left","circle_right"};
  const char* value=std::getenv("NEBBY_KEYBINDS");if(!value)return;
  std::string config(value);size_t start=0;
  while(start<config.size()){size_t end=config.find(';',start);if(end==std::string::npos)end=config.size();const auto entry=config.substr(start,end-start);const auto equals=entry.find('=');
   if(equals!=std::string::npos){const auto name=entry.substr(0,equals),key=entry.substr(equals+1);const auto code=SDL_GetScancodeFromName(key.c_str());
    if(code!=SDL_SCANCODE_UNKNOWN){for(size_t i=0;i<16;++i)if(name==names[i])keys[i]=code;}else std::fprintf(stderr,"NEBBY_KEYBIND_REJECT key=%s\n",key.c_str());
   }start=end+1;
  }
  std::fprintf(stderr,"NEBBY_KEYBINDS_ACTIVE=%s\n",value);
 }
};
inline const KeyboardMap& Keymap(){static KeyboardMap map;return map;}
inline uint32_t Button(SDL_Scancode code){uint32_t bits=0;for(unsigned i=0;i<12;++i)if(Keymap().keys[i]==code)bits|=1u<<i;return bits;}
inline std::atomic<uint32_t> Circle{0};
inline uint32_t Direction(SDL_Scancode code){uint32_t bits=0;for(unsigned i=0;i<4;++i)if(Keymap().keys[i+12]==code)bits|=1u<<i;return bits;}
inline int Event(void*,SDL_Event*e){
 if(e->type==SDL_WINDOWEVENT&&e->window.windowID==WindowId&&e->window.event==SDL_WINDOWEVENT_FOCUS_LOST)BattleClick=-1;
 if(e->type==SDL_KEYDOWN||e->type==SDL_KEYUP){if(e->key.windowID!=WindowId)return 1;const bool down=e->type==SDL_KEYDOWN;const auto code=e->key.keysym.scancode;
  if(Enabled()&&down&&!e->key.repeat&&code==SDL_SCANCODE_F9){DebugDual=!DebugDual.load();std::fprintf(stderr,"MOON_SINGLE_SCREEN mode=%s\n",DebugDual?"Dual-Screen Debug":"Single Screen");}
  if(Enabled()&&down&&!e->key.repeat&&code==SDL_SCANCODE_F10){ManualPanel=!ManualPanel.load();}
  const auto button=Button(code),direction=Direction(code);if(down){if(!e->key.repeat){PendingButtons.fetch_or(button&~Held.load());PendingCircle.fetch_or(direction&~Circle.load());}Held.fetch_or(button);Circle.fetch_or(direction);}else{Held.fetch_and(~button);Circle.fetch_and(~direction);}
 }else if(e->type==SDL_MOUSEMOTION&&e->motion.windowID==WindowId){MouseX=e->motion.x;MouseY=e->motion.y;}
 else if((e->type==SDL_MOUSEBUTTONDOWN||e->type==SDL_MOUSEBUTTONUP)&&e->button.windowID==WindowId&&e->button.button==SDL_BUTTON_LEFT){MouseX=e->button.x;MouseY=e->button.y;MouseDown=e->type==SDL_MOUSEBUTTONDOWN;
  if(e->type==SDL_MOUSEBUTTONDOWN&&BattleRootActive&&!DebugDual&&!ManualPanel){
   auto hit=MoonBattleRoot::Hit(float(WindowWidth.load()),float(WindowHeight.load()),float(e->button.x),float(e->button.y));
   if(hit){const int id=BattleCommands[unsigned(*hit)].load();if(id>=0&&id<4)BattleClick=id;}
  }
 }
 else if(e->type==SDL_WINDOWEVENT&&e->window.windowID==WindowId){if(e->window.event==SDL_WINDOWEVENT_FOCUS_LOST){Held=0;Circle=0;PendingButtons=0;PendingCircle=0;MouseDown=false;}if(e->window.event==SDL_WINDOWEVENT_SIZE_CHANGED){WindowWidth=e->window.data1;WindowHeight=e->window.data2;}}
 return 1;
}
inline void Attach(SDL_Window*w){WindowId=SDL_GetWindowID(w);int width,height;SDL_GetWindowSize(w,&width,&height);WindowWidth=width;WindowHeight=height;SDL_AddEventWatch(Event,nullptr);std::fprintf(stderr,"NATIVE_INPUT_WINDOW=TriAevum size=%dx%d single_screen=%d\n",width,height,Enabled());}
struct HostInput{uint32_t Buttons;float CircleX,CircleY;int32_t TouchX,TouchY;uint32_t TouchDown;};
static_assert(sizeof(HostInput)==24,"Keep the original input snapshot ABI; edges use the optional export");
inline HostInput Input(){const auto pressedCircle=PendingCircle.exchange(0),pressedButtons=PendingButtons.exchange(0);SamplePressEdges=uint64_t(pressedButtons)|(uint64_t(pressedCircle)<<32);const auto c=Circle.load()|pressedCircle;HostInput i{Held.load()|pressedButtons,float(bool(c&8))-float(bool(c&4)),float(bool(c&1))-float(bool(c&2)),0,0,0};
 if(MouseDown&&(!BattleRootActive||DebugDual||ManualPanel)&&(Current()==DualScreen||Current()==BottomPanel||Current()==BattleOverlay)){
  auto r=Fast::Renderer3ds::FitScreenSurface(WindowWidth,WindowHeight,320,240,BottomRegion());
  auto p=Fast::Renderer3ds::WindowToSurface(MouseX,MouseY,r,320,240);if(p){i.TouchX=static_cast<int32_t>(p->X);i.TouchY=static_cast<int32_t>(p->Y);i.TouchDown=1;}
 }return i;
}
}
