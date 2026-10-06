/* Moon EUR v1.0 semantic camera adapter and optional ownership diagnostics.
 * Camera writes are confined to verified free-roam game camera state. */
#include "recomp.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include "../../core/modern_camera.h"

extern uint32_t m040_base;
static struct { float x,y; uint32_t recenter; double seconds; } host_input;
static float mouse_x,mouse_y;
static double last_camera_time;
static int camera_active;
static uint32_t field_manager;
static uint32_t field_player;
static uint32_t recenter_pending;
void modern_camera_host_recenter(uint32_t held) {
    static uint32_t previous;
    if (held && !previous) recenter_pending=1;
    previous=held;
}
static float preference(const char *name,float fallback,float minimum,float maximum) {
    const char *text=getenv(name);if (!text) return fallback;
    char *end;float value=strtof(text,&end);
    if (end==text || *end || !isfinite(value) || value<minimum || value>maximum) {
        fprintf(stderr,"MODERN_CAMERA_SETTING_REJECT %s\n",name);return fallback;
    }
    return value;
}
uint32_t modern_camera_enabled(void) {
    static int mode=-1;
    if (mode<0) {
        const char *value=getenv("NEBBY_MODERN_CAMERA_FOLLOW_PROBE");
        mode=value && !strcmp(value,"1");
    }
    return mode;
}
uint32_t modern_camera_capture_requested(void) {
    return camera_active && host_input.seconds >= last_camera_time &&
        host_input.seconds-last_camera_time < 0.1;
}
void modern_camera_host_mouse(float x,float y,uint32_t captured) {
    if (captured) { mouse_x+=x;mouse_y+=y; }
    else { mouse_x=0;mouse_y=0; }
}
/* Called by the same cooperative execution thread as game camera updates. */
void modern_camera_host_stick(float x,float y,uint32_t recenter,double seconds) {
    if (recenter && !host_input.recenter) recenter_pending=1;
    host_input.x=x;host_input.y=y;host_input.recenter=recenter;host_input.seconds=seconds;
}
static int enabled(void) {
    static int mode = -1;
    if (mode < 0) {
        const char *value = getenv("NEBBY_MODERN_CAMERA_TRACE");
        mode = value && !strcmp(value, "1");
    }
    return mode;
}
static int vector(Context *ctx, uint32_t address, float out[3]) {
    if (!ctx->read_pages || address > UINT32_MAX - 11) return 0;
    for (unsigned i = 0; i < 12; ++i) {
        uint32_t a = address + i;
        const uint8_t *page = ctx->read_pages[a >> 12];
        if (!page) return 0;
        ((uint8_t *)out)[i] = page[a & 4095];
    }
    return 1;
}
static int field_word(Context *ctx,uint32_t object,uint32_t offset,uint32_t *out) {
    float bytes[3];
    if (!object || object>UINT32_MAX-offset-11 || !vector(ctx,object+offset,bytes)) return 0;
    memcpy(out,bytes,4);return 1;
}
static int free_field(Context *ctx) {
    uint32_t main,view,unit,active,camera,controller,target,vtable,method,player,events,actions,animation;
    if (!field_word(ctx,field_manager,4,&main) || !field_word(ctx,field_manager,8,&view) ||
        !field_word(ctx,field_manager,12,&unit) || !field_word(ctx,unit,4,&active) ||
        !field_word(ctx,unit,8,&camera) || !field_word(ctx,unit,12,&controller) ||
        controller!=ctx->r[4] || camera!=ctx->r[0] ||
        !field_word(ctx,controller,0xF4,&target) || !field_word(ctx,target,0,&vtable) ||
        !field_word(ctx,vtable,0x30,&method) || method!=m040_base+0x87550 ||
        !field_word(ctx,target,0x150,&player) || !field_word(ctx,player,0xD8,&events) ||
        !field_word(ctx,player,0xE0,&actions) || !field_word(ctx,camera,0x68,&animation)) return 0;
    int safe=main==0 && (view==UINT32_MAX || view==0) && active && !events && !actions && !(animation&255);
    field_player=safe?player:0;
    static int previous=-1;
    if (enabled() && previous!=safe) {
        fprintf(stderr,"CAMERA_CONTEXT free=%d manager=%08x main=%u view=%d unit=%08x player=%08x events=%u actions=%u animation=%u\n",safe,field_manager,main,(int32_t)view,unit,player,events,actions,animation&255);
        previous=safe;
    }
    return safe;
}
static int player_heading(Context *ctx,float *heading) {
    uint32_t model,node,w;
    float q[4];
    if (!field_word(ctx,field_player,0xF8,&model) || !field_word(ctx,model,4,&node) ||
        node>UINT32_MAX-0x50 || !vector(ctx,node+0x38,q) ||
        !field_word(ctx,node,0x44,&w)) return 0;
    memcpy(&q[3],&w,4);
    float norm=0;
    for (unsigned i=0;i<4;++i) { if (!isfinite(q[i])) return 0;norm+=q[i]*q[i]; }
    if (fabsf(norm-1)>0.05f) return 0;
    *heading=atan2f(2*(q[0]*q[2]+q[3]*q[1]),1-2*(q[0]*q[0]+q[1]*q[1]));
    return 1;
}
extern void m040_f_0006FC94(Context *);
static int moon_collision(void *user,McVec3 start,McVec3 end,float radius,float *fraction) {
    Context *ctx=user;
    (void)radius; /* Thin ray first; swept radius remains pending. */
    uint32_t scenes[2];
    if (!field_word(ctx,field_player,0x39C,&scenes[0]) ||
        !field_word(ctx,field_player,0x3A0,&scenes[1]) || ctx->r[13]<4112) return -1;
    uint32_t scratch=(ctx->r[13]-4096)&~15u;
    uint8_t saved[4096];
    for (unsigned i=0;i<4096;++i) {
        uint32_t a=scratch+i;
        if (!ctx->read_pages[a>>12] || !ctx->write_pages[a>>12]) return -1;
        saved[i]=ctx->read_pages[a>>12][a&4095];
    }
    float from[4]={start.x,start.y,start.z,0},to[4]={end.x,end.y,end.z,0};
    uint32_t source=scratch+3500,destination=scratch+3520,output=scratch+3552;
    for (unsigned i=0;i<16;++i) {
        uint32_t a=source+i;ctx->write_pages[a>>12][a&4095]=((uint8_t *)from)[i];
        a=destination+i;ctx->write_pages[a>>12][a&4095]=((uint8_t *)to)[i];
    }
    int result=0,queries=0;
    float nearest=1;
    for (unsigned scene=0;scene<2;++scene) {
        if (!scenes[scene] || (scene && scenes[scene]==scenes[0])) continue;
        uint32_t vfp[32],fpscr=*ctx->fpscr;
        memcpy(vfp,ctx->vfp,sizeof(vfp));
        Context call=*ctx;call.vfp=vfp;call.fpscr=&fpscr;
        call.r[0]=scenes[scene];call.r[1]=source;call.r[2]=destination;call.r[3]=output;
        call.r[13]=scratch+3072;call.r[14]=0xFFFFFFF0;call.r[15]=m040_base+0x6FC94;
        call.exit=0;call.depth=0;call.budget=1000000;call.thumb=0;
        m040_f_0006FC94(&call);
        if (call.exit || call.r[15]!=0xFFFFFFF0) {result=-1;break;}
        ++queries;
        if (call.r[0]) {
            float hit[3];
            if (!vector(ctx,output,hit)) {result=-1;break;}
            McVec3 d={end.x-start.x,end.y-start.y,end.z-start.z};
            float denominator=d.x*d.x+d.y*d.y+d.z*d.z;
            float t=((hit[0]-start.x)*d.x+(hit[1]-start.y)*d.y+(hit[2]-start.z)*d.z)/denominator;
            if (!isfinite(t) || t<0 || t>1) {result=-1;break;}
            nearest=fminf(nearest,t);result=1;
        }
    }
    for (unsigned i=0;i<4096;++i) {
        uint32_t a=scratch+i;ctx->write_pages[a>>12][a&4095]=saved[i];
    }
    if (!queries) return -1;
    if (result==1) *fraction=nearest;
    static int first;
    if (!first && enabled()) {
        fprintf(stderr,"MODERN_CAMERA_COLLISION_QUERY ground=%08x wall=%08x result=%d fraction=%f\n",scenes[0],scenes[1],result,nearest);first=1;
    }
    return result;
}
/* Explicit stage-two experiment, not the finished mod toggle. The final
 * adapter must additionally prove free-roam ownership before enabling this. */
static void follow_probe(Context *ctx) {
    static uint32_t camera;
    static McFollow state;
    static double previous_time;
    if (!modern_camera_enabled() || ctx->r[15] != 0x0040D2F8 || !m040_base ||
        ctx->r[14] != m040_base + 0x33AFC) return;
    if (!free_field(ctx)) { camera_active=0;recenter_pending=0;previous_time=0;mc_follow_reset(&state);return; }
    camera_active=1;last_camera_time=host_input.seconds;
    float focus[3];
    if (!vector(ctx, ctx->r[2], focus) || !ctx->write_pages) return;
    /* Validate every destination before altering any byte. These are the
     * controller's live stack vectors consumed by BaseCamera::LookAt. */
    for (unsigned arg = 1; arg <= 2; ++arg) {
        uint32_t address = ctx->r[arg];
        if (address > UINT32_MAX - 11) return;
        for (unsigned i = 0; i < 12; ++i)
            if (!ctx->write_pages[(address + i) >> 12]) return;
    }
    if (camera != ctx->r[0]) { mc_follow_reset(&state); camera = ctx->r[0]; }
    /* ~606 units observed retail camera distance in the field trace. */
    static McFollowConfig config = {600, 0, 0.22f, 0.5f, 0.08f, 0};
    static int configured,invert,collision;
    static float mouse_speed,stick_speed,fov;
    if (!configured) {
        config.distance=preference("NEBBY_MODERN_CAMERA_DISTANCE",600,80,1200);
        config.height=preference("NEBBY_MODERN_CAMERA_HEIGHT",0,-100,300);
        config.smoothing_seconds=preference("NEBBY_MODERN_CAMERA_SMOOTHING",0.08f,0,0.5f);
        mouse_speed=0.0025f*preference("NEBBY_MODERN_CAMERA_MOUSE",1,0.05f,5);
        stick_speed=2*preference("NEBBY_MODERN_CAMERA_STICK",1,0.05f,5);
        invert=preference("NEBBY_MODERN_CAMERA_INVERT_Y",0,0,1)!=0;
        collision=preference("NEBBY_MODERN_CAMERA_COLLISION",0,0,1)!=0 || getenv("NEBBY_MODERN_CAMERA_COLLISION_PROBE");
        fov=preference("NEBBY_MODERN_CAMERA_FOV",0,25,85)*0.01745329252f;
        config.fov_y=fov;
        configured=1;
    }
    float seconds = previous_time && host_input.seconds >= previous_time ?
        (float)(host_input.seconds-previous_time) : 0;
    previous_time=host_input.seconds;
    if (seconds > 0.1f) seconds=0.1f;
    if (recenter_pending) {
        float heading;
        if (player_heading(ctx,&heading)) {
            config.yaw=remainderf(heading+3.14159265359f,6.28318530718f);
            if (enabled()) fprintf(stderr,"MODERN_CAMERA_RECENTER player=%08x heading=%f yaw=%f\n",field_player,heading,config.yaw);
        }
        recenter_pending=0;
    }
    mc_orbit_input(&config,host_input.x,host_input.y,mouse_x,mouse_y,seconds,stick_speed,mouse_speed,0.15f,invert);
    mouse_x=0;mouse_y=0;
    if (!state.initialized) {
        float original[3];
        if (!vector(ctx,ctx->r[1],original)) return;
        state.pose=(McPose){.position={original[0],original[1],original[2]},
            .target={focus[0],focus[1],focus[2]},.up={0,1,0}};
        uint32_t bits;
        if (field_word(ctx,ctx->r[0],0xAC,&bits)) memcpy(&state.pose.fov_y,&bits,4);
        state.initialized=1;
    }
    McPose pose;
    if (!mc_follow_step(&state, &config, (McVec3){focus[0],focus[1],focus[2]}, seconds, &pose)) return;
    if (collision) {
        int resolved=mc_resolve_collision_orbit(&pose,0,20,180,moon_collision,ctx);
        if (resolved<0) {
            camera_active=0;
            mc_follow_reset(&state);
            return;
        }
        if (resolved==2) config.yaw=atan2f(pose.position.x-pose.target.x,
                                         pose.position.z-pose.target.z);
        state.pose=pose;
    }
    if (fov>0) {
        uint32_t address=ctx->r[0]+0xAC;
        for (unsigned i=0;i<4;++i) if (!ctx->write_pages[(address+i)>>12]) return;
        for (unsigned i=0;i<4;++i) ctx->write_pages[(address+i)>>12][(address+i)&4095]=((uint8_t *)&pose.fov_y)[i];
    }
    const McVec3 *values[2] = {&pose.position, &pose.target};
    for (unsigned arg = 1; arg <= 2; ++arg)
        for (unsigned i = 0; i < 12; ++i) {
            uint32_t address = ctx->r[arg] + i;
            ctx->write_pages[address >> 12][address & 4095] = ((const uint8_t *)values[arg-1])[i];
        }
    static unsigned writes;
    if (++writes == 1) {
        fprintf(stderr,"MODERN_CAMERA_FIXED_FOLLOW owner=%08x controller=%08x semantic=FieldRo.ControllerTypeArea.LookAt renderer_override=0 distance=%f height=%f fov=%f smoothing=%f collision=%d\n",camera,ctx->r[4],config.distance,config.height,fov,config.smoothing_seconds,collision);
    }
}
static void trace(Context *ctx, uint32_t entry, const char *name) {
    if (ctx->r[15] != entry || !enabled()) return;
    static struct { uint32_t owner, caller, entry; unsigned count; } seen[128];
    unsigned slot;
    for (slot = 0; slot < 128; ++slot) {
        if (!seen[slot].count || (seen[slot].owner == ctx->r[0] &&
            seen[slot].caller == ctx->r[14] && seen[slot].entry == entry)) break;
    }
    if (slot == 128) return;
    seen[slot].owner = ctx->r[0]; seen[slot].caller = ctx->r[14]; seen[slot].entry = entry;
    unsigned count = ++seen[slot].count;
    if (count > 3 && count % 300) return;
    float position[3], target[3];
    int p = (entry==0x0040D03C || entry==0x0040D2F8) && vector(ctx, ctx->r[1], position);
    int t = entry == 0x0040D2F8 && vector(ctx, ctx->r[2], target);
    fprintf(stderr, "CAMERA_PROBE %s owner=%08x caller=%08x field_base=%08x field_offset=%08x count=%u",
        name, ctx->r[0], ctx->r[14], m040_base, ctx->r[14] - m040_base, count);
    if (p) fprintf(stderr, " position=(%.3f,%.3f,%.3f)", position[0],position[1],position[2]);
    if (t) fprintf(stderr, " target=(%.3f,%.3f,%.3f)", target[0],target[1],target[2]);
    if (entry==0x004A3CF4 || entry==0x004A5674)
        fprintf(stderr," r4=%08x r5=%08x r6=%08x",ctx->r[4],ctx->r[5],ctx->r[6]);
    if (entry == 0x0040D2F8 && m040_base && ctx->r[14] == m040_base + 0x33AFC) {
        float bytes[3];
        uint32_t owner = ctx->r[4], target_object = 0, target_vtable = 0, target_method = 0;
        if (owner <= UINT32_MAX - 0x100 && vector(ctx, owner + 0xF4, bytes)) {
            memcpy(&target_object, bytes, 4);
            if (vector(ctx, target_object, bytes)) memcpy(&target_vtable, bytes, 4);
            if (target_vtable <= UINT32_MAX-0x3C && vector(ctx, target_vtable+0x30, bytes)) memcpy(&target_method,bytes,4);
        }
        fprintf(stderr, " area_controller=%08x target_object=%08x target_vtable=%08x target_method=%08x", owner, target_object, target_vtable,target_method);
    }
    fputc('\n', stderr);
}
#define PROBE(symbol, address, name) \
    extern void __real_##symbol(Context *); \
    void __wrap_##symbol(Context *ctx) { trace(ctx,address,name); __real_##symbol(ctx); }
PROBE(f_0040D03C, 0x0040D03C, "BaseCamera.SetPosition")
PROBE(f_004A3CF4, 0x004A3CF4, "BaseCamera.GetRotationQuat")
PROBE(f_004A5674, 0x004A5674, "BaseModel.GetRotationQuat")
PROBE(f_0036C140, 0x0036C140, "CollisionModel.GetIntersection")
extern void __real_m040_f_0005BCD0(Context *);
extern void __real_m040_f_0006FC94(Context *);
void __wrap_m040_f_0006FC94(Context *ctx) {
    trace(ctx,m040_base+0x6FC94,"BaseCollisionScene.RaycastFromStaticActorsMesh");
    __real_m040_f_0006FC94(ctx);
}
void __wrap_m040_f_0005BCD0(Context *ctx) {
    trace(ctx,m040_base+0x5BCD0,"CameraUnit.GetRotationY");
    __real_m040_f_0005BCD0(ctx);
}
extern void __real_m040_f_00035DEC(Context *);
void __wrap_m040_f_00035DEC(Context *ctx) {
    trace(ctx,m040_base+0x35DEC,"UiDeviceTranslator.CameraAngleCheckUpdate");
    /* Retail DIRECT mode returns true before touching the stick log.
       Refresh heading while orbiting, without changing the translator's mode. */
    uint32_t manager,events,actions;
    static int relative=-1;
    if (relative<0) relative=preference("NEBBY_MODERN_CAMERA_RELATIVE",1,0,1)!=0;
    if (relative && ctx->r[15]==m040_base+0x35DEC && !ctx->thumb && camera_active &&
        host_input.seconds-last_camera_time<0.1 &&
        field_word(ctx,ctx->r[0],0x1C,&manager) && manager==field_manager &&
        field_word(ctx,field_player,0xD8,&events) && !events &&
        field_word(ctx,field_player,0xE0,&actions) && !actions) {
        static int reported;
        if (!reported && enabled()) {
            fprintf(stderr,"MODERN_CAMERA_RELATIVE_MOVEMENT translator=%08x manager=%08x check=FieldRo+35DEC\n",ctx->r[0],manager);
            reported=1;
        }
        ctx->r[0]=1;
        ctx->r[15]=ctx->r[14];
        return;
    }
    __real_m040_f_00035DEC(ctx);
}
extern void __real_f_0040D2F8(Context *);
void __wrap_f_0040D2F8(Context *ctx) {
    trace(ctx, 0x0040D2F8, "BaseCamera.SetupCameraLookAt");
    if (enabled() && ctx->r[15]==0x0040D2F8 && ctx->r[14]==m040_base+0x33AFC) (void)free_field(ctx);
    follow_probe(ctx);
    __real_f_0040D2F8(ctx);
}
extern void __real_m040_f_000338B0(Context *);
void __wrap_m040_f_000338B0(Context *ctx) {
    static unsigned samples;
    if (modern_camera_enabled() && ctx->r[15]==m040_base+0x338B0 && ctx->r[14]==m040_base+0x36604)
        field_manager=ctx->r[5];
    if (enabled() && ctx->r[15] == m040_base+0x338B0 && samples++ < 3)
        fprintf(stderr,"CAMERA_AREA_UPDATE controller=%08x camera=%08x caller=%08x r4=%08x r5=%08x r6=%08x\n",ctx->r[0],ctx->r[1],ctx->r[14],ctx->r[4],ctx->r[5],ctx->r[6]);
    __real_m040_f_000338B0(ctx);
}
