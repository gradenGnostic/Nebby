#include <stdint.h>
#include <string.h>
#include "overrides.h"

extern uint32_t pc_input_buttons(uint32_t phase);
extern uint32_t pc_input_vector(uint32_t kind, uint32_t phase);
extern float pc_input_axis(uint32_t kind, uint32_t axis);
extern void pc_input_trace(uint32_t event, uint32_t value);

enum { HELD, PRESSED, RELEASED, REPEAT };
enum { CROSS, STICK };
static uint32_t buttons[64], vectors[2][64];
static unsigned button_count, vector_count[2];

static void remember(uint32_t *objects, unsigned *count, uint32_t object) {
    if (!object) return;
    for (unsigned i = 0; i < *count; i++) if (objects[i] == object) return;
    if (*count < 64) objects[(*count)++] = object;
}

static int button_known(uint32_t object) {
    for (unsigned i = 0; i < button_count; i++) if (buttons[i] == object) return 1;
    return 0;
}

static int vector_kind(uint32_t object) {
    for (unsigned kind = 0; kind < 2; kind++)
        for (unsigned i = 0; i < vector_count[kind]; i++)
            if (vectors[kind][i] == object) return (int)kind;
    return -1;
}

/* Also publish the recovered game-facing vector state, for inlined getters in
   CRO callers. These are GFL2 device buffers, not SDK HID/shared memory. */
static void publish_vector(Context *ctx, uint32_t object, unsigned kind) {
    uint32_t xy[2];
    for (unsigned axis=0;axis<2;axis++) {
        float value=pc_input_axis(kind,axis);
        memcpy(&xy[axis],&value,4);
    }
    const unsigned offsets[3]={0x30,0x34,0x44};
    for (unsigned slot=0;slot<3;slot++) {
        uint32_t state=mem_read32(ctx,object+offsets[slot]);
        if (!state) continue;
        mem_write32(ctx,state,xy[0]);mem_write32(ctx,state+4,xy[1]);
        for(unsigned phase=0;phase<4;phase++)
            mem_write32(ctx,state+8+phase*4,pc_input_vector(kind,phase));
    }
}

/* GFL2 Button bits differ from hid:USER bits. Retail AddButtonAssignment
   stores each physical button's virtual button mask at object+0x48+4*bit. */
static uint32_t native_button_mask(Context *ctx, uint32_t object, uint32_t phase) {
    static const uint8_t physical_bit[12] = {
        4, 5, 12, 13, 1, 0, 2, 3, 9, 8, 6, 7
    };
    uint32_t host = pc_input_buttons(phase);
    uint32_t result = 0;
    for (unsigned bit = 0; bit < 12; bit++) {
        if (host & (1u << bit)) {
            unsigned key = physical_bit[bit];
            result |= (1u << key) | mem_read32(ctx, object + 0x48u + 4u * key);
        }
    }
    return result;
}

static void bool_result(Context *ctx, uint32_t state, uint32_t mask) {
    ctx->r[0] = (state & mask) != 0;
    RETURN_TO(ctx->r[14]);
}

RECOMP_OVERRIDE(0x001049C8) {
    uint32_t index = ctx->r[1] & 0xff;
    CALL(RECOMP_ORIGINAL(0x001049C8));
    if (index < 3) { remember(buttons, &button_count, ctx->r[0]); pc_input_trace(0, ctx->r[0]); }
}

RECOMP_OVERRIDE(0x00498930) {
    uint32_t index = ctx->r[1] & 0xff;
    CALL(RECOMP_ORIGINAL(0x00498930));
    if (index < 3) { remember(vectors[CROSS], &vector_count[CROSS], ctx->r[0]); pc_input_trace(1, ctx->r[0]); if(ctx->r[0])publish_vector(ctx,ctx->r[0],CROSS); }
}

RECOMP_OVERRIDE(0x004989E4) {
    uint32_t index = ctx->r[1] & 0xff;
    CALL(RECOMP_ORIGINAL(0x004989E4));
    if (index < 3) { remember(vectors[STICK], &vector_count[STICK], ctx->r[0]); pc_input_trace(2, ctx->r[0]); if(ctx->r[0])publish_vector(ctx,ctx->r[0],STICK); }
}

RECOMP_OVERRIDE(0x00498F08) {
    if (!button_known(ctx->r[0])) { CALL(RECOMP_ORIGINAL(0x00498F08)); return; }
    ctx->r[0] = native_button_mask(ctx, ctx->r[0], HELD);
    RETURN_TO(ctx->r[14]);
}

RECOMP_OVERRIDE(0x001049F8) {
    if (!button_known(ctx->r[0])) { CALL(RECOMP_ORIGINAL(0x001049F8)); return; }
    bool_result(ctx, native_button_mask(ctx, ctx->r[0], HELD), ctx->r[1]);
}

RECOMP_OVERRIDE(0x00498F40) {
    if (!button_known(ctx->r[0])) { CALL(RECOMP_ORIGINAL(0x00498F40)); return; }
    bool_result(ctx, native_button_mask(ctx, ctx->r[0], REPEAT), ctx->r[1]);
}

RECOMP_OVERRIDE(0x00498F90) {
    if (!button_known(ctx->r[0])) { CALL(RECOMP_ORIGINAL(0x00498F90)); return; }
    bool_result(ctx, native_button_mask(ctx, ctx->r[0], RELEASED), ctx->r[1]);
}

RECOMP_OVERRIDE(0x00498FD8) {
    if (!button_known(ctx->r[0])) { CALL(RECOMP_ORIGINAL(0x00498FD8)); return; }
    pc_input_trace(3, ctx->r[0]);
    if (pc_input_buttons(HELD) & 1) {
        pc_input_trace(4, ctx->r[1]);
        pc_input_trace(5, native_button_mask(ctx, ctx->r[0], HELD));
        pc_input_trace(6, native_button_mask(ctx, ctx->r[0], PRESSED));
    }
    bool_result(ctx, native_button_mask(ctx, ctx->r[0], PRESSED), ctx->r[1]);
}

RECOMP_OVERRIDE(0x004983E8) {
    int kind = vector_kind(ctx->r[0]);
    if (kind < 0) { CALL(RECOMP_ORIGINAL(0x004983E8)); return; }
    ctx->r[0] = pc_input_vector((uint32_t)kind, PRESSED);
    RETURN_TO(ctx->r[14]);
}

RECOMP_OVERRIDE(0x0049868C) {
    int kind = vector_kind(ctx->r[0]);
    if (kind < 0) { CALL(RECOMP_ORIGINAL(0x0049868C)); return; }
    float value = pc_input_axis((uint32_t)kind, 0);
    memcpy(&ctx->vfp[0], &value, sizeof(value));
    RETURN_TO(ctx->r[14]);
}

RECOMP_OVERRIDE(0x0049869C) {
    int kind = vector_kind(ctx->r[0]);
    if (kind < 0) { CALL(RECOMP_ORIGINAL(0x0049869C)); return; }
    float value = pc_input_axis((uint32_t)kind, 1);
    memcpy(&ctx->vfp[0], &value, sizeof(value));
    RETURN_TO(ctx->r[14]);
}

RECOMP_OVERRIDE(0x00498704) {
    int kind = vector_kind(ctx->r[0]);
    if (kind < 0) { CALL(RECOMP_ORIGINAL(0x00498704)); return; }
    bool_result(ctx, pc_input_vector((uint32_t)kind, HELD), ctx->r[1]);
}

RECOMP_OVERRIDE(0x00498720) {
    int kind = vector_kind(ctx->r[0]);
    if (kind < 0) { CALL(RECOMP_ORIGINAL(0x00498720)); return; }
    ctx->r[0] = pc_input_vector((uint32_t)kind, HELD);
    RETURN_TO(ctx->r[14]);
}

RECOMP_OVERRIDE(0x00498730) {
    int kind = vector_kind(ctx->r[0]);
    if (kind < 0) { CALL(RECOMP_ORIGINAL(0x00498730)); return; }
    bool_result(ctx, pc_input_vector((uint32_t)kind, REPEAT), ctx->r[1]);
}

RECOMP_OVERRIDE(0x00498754) {
    int kind = vector_kind(ctx->r[0]);
    if (kind < 0) { CALL(RECOMP_ORIGINAL(0x00498754)); return; }
    bool_result(ctx, pc_input_vector((uint32_t)kind, PRESSED), ctx->r[1]);
}
