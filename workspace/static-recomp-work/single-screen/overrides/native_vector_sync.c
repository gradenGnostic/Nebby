/* EUR v1.0 GFL2 vector synchronization at the real Device::Update return.
 * Own host adapter; generated game code/archive stays untouched.
 */
#include "overrides.h"
extern void __real_override_0x001049C8(Context*);
extern void __real_override_0x00498930(Context*);
extern void __real_override_0x004989E4(Context*);
extern void __real_f_00346C98(Context*);
extern uint32_t pc_input_vector(uint32_t,uint32_t);
extern float pc_input_axis(uint32_t,uint32_t);
extern uint32_t pc_input_buttons(uint32_t);
extern void pc_input_trace(uint32_t,uint32_t);
typedef struct {uint32_t object,tag,kind,ready;} Device;
typedef struct {uint32_t tls,sp,caller,object,tag,kind;} Pending;
static Device devices[64];static unsigned device_count;
static Pending pending[64];static unsigned pending_count;
static void publish(Context*,const Pending*);
static void remember(Context*ctx,uint32_t object,uint32_t kind){
    if(!object)return;
    const uint32_t tag=mem_read32(ctx,object);
    for(unsigned i=0;i<device_count;i++)if(devices[i].object==object){
        if(devices[i].tag!=tag||devices[i].kind!=kind)devices[i]=(Device){object,tag,kind,0};
        return;
    }
    if(device_count<64){devices[device_count++]=(Device){object,tag,kind,0};pc_input_trace(kind==2?12:8+kind,object);}
}
static void register_result(Context*ctx,uint32_t kind){
    const uint32_t object=ctx->r[0];remember(ctx,object,kind);
    for(unsigned i=0;i<device_count;i++)if(devices[i].object==object&&!devices[i].ready){
        const Pending initial={0,0,0,object,devices[i].tag,kind};publish(ctx,&initial);break;
    }
}
void __wrap_override_0x001049C8(Context*ctx){
    const uint32_t index=ctx->r[1]&255,caller=ctx->r[14];
    original_0x001049C8(ctx);
    if(index<3&&!ctx->exit&&ctx->r[15]==(caller&~1u))register_result(ctx,2);
}
void __wrap_override_0x00498930(Context*ctx){
    const uint32_t index=ctx->r[1]&255,caller=ctx->r[14];
    original_0x00498930(ctx);
    if(index<3&&!ctx->exit&&ctx->r[15]==(caller&~1u))register_result(ctx,0);
}
void __wrap_override_0x004989E4(Context*ctx){
    const uint32_t index=ctx->r[1]&255,caller=ctx->r[14];
    original_0x004989E4(ctx);
    if(index<3&&!ctx->exit&&ctx->r[15]==(caller&~1u))register_result(ctx,1);
}
static void publish(Context*ctx,const Pending*p){
    // GetEffectiveData0x1054dc selects +34, +44 or +30 depending on flags.
    // Preserve all three retail buffers for direct/inlined CRO consumers.
    const unsigned offsets[]={0x30,0x34,0x44};
    uint32_t xy[2]={0},phases[4],assigned[4]={0};
    if(p->kind<2){
        for(unsigned axis=0;axis<2;axis++){float f=pc_input_axis(p->kind,axis);memcpy(&xy[axis],&f,4);}
        for(unsigned phase=0;phase<4;phase++)phases[phase]=pc_input_vector(p->kind,phase);
    }else{
        // Button::GetHold/IsTrigger select original(+0) or assigned(+16)
        // 16-byte states. Never merge the two masks: retail preserves them.
        static const uint8_t physical_bit[12]={4,5,12,13,1,0,2,3,9,8,6,7};
        for(unsigned phase=0;phase<4;phase++){
            const uint32_t host=pc_input_buttons(phase);phases[phase]=0;
            for(unsigned bit=0;bit<12;bit++)if(host&(1u<<bit)){
                const unsigned key=physical_bit[bit];phases[phase]|=1u<<key;
                assigned[phase]|=mem_read32(ctx,p->object+0x48+key*4);
            }
        }
    }
    for(unsigned i=0;i<3;i++){
        const uint32_t state=mem_read32(ctx,p->object+offsets[i]);
        if(!state)continue;
        if(p->kind<2){
            mem_write32(ctx,state,xy[0]);mem_write32(ctx,state+4,xy[1]);
            for(unsigned phase=0;phase<4;phase++)mem_write32(ctx,state+8+4*phase,phases[phase]);
        }else for(unsigned phase=0;phase<4;phase++){
            mem_write32(ctx,state+4*phase,phases[phase]);
            mem_write32(ctx,state+16+4*phase,assigned[phase]);
        }
    }
    for(unsigned i=0;i<device_count;i++)if(devices[i].object==p->object&&devices[i].tag==p->tag)devices[i].ready=1;
    pc_input_trace(p->kind==2?13:10+p->kind,p->object);
}

// Once the actual game update publishes a device, preserve retail getters'
// flags, original/assigned selection, and repeat-or-trigger semantics. Reads
// see the same stable native snapshot as inlined CRO reads of the buffers.
static int synchronized(Context*ctx,uint32_t kind){
    for(unsigned i=0;i<device_count;i++)if(devices[i].object==ctx->r[0]&&devices[i].ready&&
        (kind==2?devices[i].kind==2:devices[i].kind<2)&&mem_read32(ctx,ctx->r[0])==devices[i].tag)return 1;
    return 0;
}
#define SYNC_GETTER(address,kind) \
    extern void __real_override_0x##address(Context*); \
    void __wrap_override_0x##address(Context*ctx){ \
        if(synchronized(ctx,kind))original_0x##address(ctx);else __real_override_0x##address(ctx); \
    }
SYNC_GETTER(004983E8,0)
SYNC_GETTER(0049868C,0)
SYNC_GETTER(0049869C,0)
SYNC_GETTER(00498704,0)
SYNC_GETTER(00498720,0)
SYNC_GETTER(00498730,0)
SYNC_GETTER(00498754,0)
SYNC_GETTER(00498F08,2)
SYNC_GETTER(001049F8,2)
SYNC_GETTER(00498F40,2)
SYNC_GETTER(00498F90,2)
SYNC_GETTER(00498FD8,2)
void __wrap_f_00346C98(Context*ctx){
    if(ctx->r[15]==0x00346C98&&!ctx->thumb){
        for(unsigned i=0;i<device_count;i++){
            const Device*d=&devices[i];
            if(d->object!=ctx->r[0]||mem_read32(ctx,d->object)!=d->tag)continue;
            unsigned slot=0;
            while(slot<pending_count&&(pending[slot].tls!=ctx->tls||pending[slot].sp!=ctx->r[13]))slot++;
            if(slot==pending_count&&pending_count<64)pending_count++;
            if(slot<64)pending[slot]=(Pending){ctx->tls,ctx->r[13],ctx->r[14],d->object,d->tag,d->kind};
            break;
        }
    }
    __real_f_00346C98(ctx);
    // AOT can yield inside Update or one of its callees. Keep the pending
    // record across yields and thread switches, publish only on real return.
    if(ctx->exit)return;
    for(unsigned i=0;i<pending_count;i++){
        const Pending p=pending[i];
        if(p.tls!=ctx->tls||p.sp!=ctx->r[13]||(p.caller&~1u)!=ctx->r[15])continue;
        if(mem_read32(ctx,p.object)==p.tag)publish(ctx,&p);
        pending[i]=pending[--pending_count];break;
    }
}
