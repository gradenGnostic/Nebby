#include "oot3d_native_pica_frontend.h"
#include "oot3d_ctr_pica_backend.h"
#include "oot3d_native_pica_submission.h"
#include "oot3d_native_pica_vulkan_plan.h"
#include "oot3d_native_pica_vulkan_bridge.h"
#include "fast/renderer3ds/pica_render_backend.h"
#include "fast/backends/gfx_sdl.h"
#include "fast/backends/gfx_vulkan.h"
#include "fast/oot3d/graphics_settings_runtime.h"
#include "moon_single_screen.h"
#include "moon_battle_root_draw.h"
#include "ship/Context.h"

#include <cstdint>
#include <cstring>
#include <mutex>
#include <string>
#include <vector>
#include <memory>
#include <cstdlib>
#include <cstdio>
#include <filesystem>
#include <unordered_set>

namespace {
struct MoonSink final : Oot3dNativeGame::Oot3dPicaPacketSink {
    bool ReadCommandList(uint32_t address, uint32_t size, std::vector<uint32_t>& words) override;
    uint64_t draws = 0;
    uint64_t rejected = 0;
    Oot3dNativeGame::Oot3dNativePicaSubmissionQueue* queue = nullptr;
    bool SubmitHardwareRegisterWrite(
        const Oot3dNativeGame::Oot3dPicaHardwareRegisterWrite&,
        std::string*) override { return true; }
    bool SubmitDrawPacket(const Oot3dNativeGame::Oot3dPicaDrawPacket& packet,
                          std::string*) override {
        ++draws;
        std::string error;
        if (queue != nullptr && !queue->SubmitDrawPacket(packet, &error))
            ++rejected;
        return true;
    }
};
using GuestRead = bool (*)(void*, uint32_t, uint8_t*, size_t);
struct MoonRuntime {
    MoonSink sink;
    void* memoryContext = nullptr;
    GuestRead guestRead = nullptr;
    std::vector<std::vector<uint8_t>> borrowedReads;
    Oot3dNativeGame::Oot3dNativePicaSubmissionQueue queue;
    Oot3dNativeGame::Oot3dNativePicaFrontend frontend;
    Oot3dSourceRuntime::CtrPicaBackend ctrBackend;
    uint64_t resolvedDraws = 0;
    uint64_t plannedDraws = 0;
    uint64_t rejectedPlans = 0;
    uint64_t picaViews = 0;
    uint64_t vertexBuffers = 0;
    uint64_t indexBuffers = 0;
    uint64_t textures = 0;
    Oot3dNativeGame::Oot3dPicaVulkanShaderSourceCache shaderCache;
    std::unique_ptr<Fast::GfxWindowBackendSDL2> nativeWindow;
    std::unique_ptr<Fast::GfxRenderingAPIVulkan> nativeBackend;
    bool backendReady = false;
    bool frameStarted = false;
    bool framePresented = false;
    uint64_t submitAttempts = 0;
    uint64_t submitSuccess = 0;
    uint64_t submitRejected = 0;
    uint64_t transferCount = 0;
    uint64_t framesStarted = 0;
    uint64_t framesCompleted = 0;
    uint64_t framesPresented = 0;
    std::vector<Fast::Renderer3ds::PicaMemoryFillView> pendingFills;
    std::unordered_set<uint64_t> dumpedPrograms;
    ImGuiContext* battleOverlayContext = nullptr;
    MoonBattleRoot::CommandMap battleCommandMap;
    int battleSelected = -1;
    bool battleRootActive = false;

    void DrawBattleRoot() {
        if (!battleRootActive || !battleCommandMap.Verified() ||
            !MoonSingleScreen::Enabled() || MoonSingleScreen::DebugDual ||
            MoonSingleScreen::ManualPanel) return;
        const auto previous = ImGui::GetCurrentContext();
        if (!battleOverlayContext) {
            battleOverlayContext = ImGui::CreateContext();
            ImGui::SetCurrentContext(battleOverlayContext);
            ImGui::GetIO().IniFilename = nullptr;
            ImGui::GetIO().LogFilename = nullptr;
        }
        ImGui::SetCurrentContext(battleOverlayContext);
        if (!nativeBackend->InitImGuiBackend()) {
            ImGui::SetCurrentContext(previous);
            return;
        }
        auto& io = ImGui::GetIO();
        io.DisplaySize = ImVec2(float(MoonSingleScreen::WindowWidth.load()),
                               float(MoonSingleScreen::WindowHeight.load()));
        io.DeltaTime = 1.f / 60.f;
        nativeBackend->NewImGuiFrame();
        ImGui::NewFrame();
        const auto hover = MoonBattleRoot::Hit(io.DisplaySize.x,io.DisplaySize.y,
                    float(MoonSingleScreen::MouseX.load()),float(MoonSingleScreen::MouseY.load()));
        MoonBattleRoot::Draw(*ImGui::GetForegroundDrawList(),io.DisplaySize.x,
                            io.DisplaySize.y,battleCommandMap,battleSelected,hover);
        ImGui::Render();
        nativeBackend->RenderImGuiDrawData(ImGui::GetDrawData());
        ImGui::SetCurrentContext(previous);
    }

    MoonRuntime()
        : queue(Oot3dNativeGame::Oot3dPicaPhysicalMemoryView(
              {{0, 0, uint64_t{1} << 32}},
              [this](uint32_t address, size_t size) -> std::span<const uint8_t> {
                  if (!guestRead || size == 0 || size > (64U << 20)) return {};
                  auto& bytes = borrowedReads.emplace_back(size);
                  if (!guestRead(memoryContext, address, bytes.data(), size)) {
                      borrowedReads.pop_back();
                      return {};
                  }
                  return bytes;
              })),
          frontend(&sink), ctrBackend(frontend) {
        sink.queue = &queue;
        // Production rendering consumes packets synchronously. Retaining every
        // register write and full shader/uniform snapshot grows to gigabytes.
        frontend.SetDiagnosticHistoryEnabled(false);
    }

    bool EnsureBackend() {
        if (backendReady) return true;
        const char* mode = std::getenv("POKEMOON_RENDERER");
        if (mode == nullptr || std::string(mode) != "triaevum") return false;
        Ship::Context::CreateUninitializedInstance("MoonTriAevum", "moontriaevum", "moontriaevum.json");
        nativeWindow = std::make_unique<Fast::GfxWindowBackendSDL2>();
        auto dimension=[](const char* name,int fallback,int minimum,int maximum){
            const char* value=std::getenv(name);if(!value)return fallback;
            char* end=nullptr;const long parsed=std::strtol(value,&end,10);
            return end!=value&&*end=='\0'&&parsed>=minimum&&parsed<=maximum?int(parsed):fallback;
        };
        int width=dimension("POKEMOON_WINDOW_WIDTH",400,320,3840);
        int height=dimension("POKEMOON_WINDOW_HEIGHT",480,240,2160);
#ifdef __ANDROID__
        SDL_SetHint(SDL_HINT_ANDROID_BLOCK_ON_PAUSE,"0");
        SDL_InitSubSystem(SDL_INIT_VIDEO);
        SDL_DisplayMode surfaceMode{};
        if(SDL_GetCurrentDisplayMode(0,&surfaceMode)==0){width=surfaceMode.w;height=surfaceMode.h;}
#endif
        // The production renderer owns a presentation-settings transaction;
        // seed it too, otherwise its first frame restores 1280x720 over SDL.
        if(std::getenv("POKEMOON_WINDOW_WIDTH")||std::getenv("POKEMOON_WINDOW_HEIGHT")
#ifdef __ANDROID__
           || true
#endif
        ){
            auto& settingsRuntime=Fast::Oot3d::GraphicsSettingsRuntime::Instance();
            auto settings=settingsRuntime.Snapshot();
            settings.OutputWidth=width;settings.OutputHeight=height;
            const auto result=settingsRuntime.Apply(settings,false);
            if(!result.Accepted())throw std::runtime_error("Native window resolution rejected");
        }
        nativeWindow->Init("Pokémon Moon", "Vulkan", false,width,height,50,50);
        SDL_SetWindowTitle(static_cast<SDL_Window*>(nativeWindow->GetNativeWindow()),"Pokémon Moon");
        MoonSingleScreen::Attach(static_cast<SDL_Window*>(nativeWindow->GetNativeWindow()));
        nativeBackend = std::make_unique<Fast::GfxRenderingAPIVulkan>(nativeWindow.get());
        nativeBackend->Init();
        nativeBackend->SetPicaPhysicalMemoryAccess(
            [this](uint32_t address, std::span<uint8_t> output) {
                return guestRead && guestRead(memoryContext, address, output.data(), output.size());
            }, {});
        backendReady = true;
        std::fprintf(stderr, "FIRST_TRIAEVUM_BACKEND_CREATED\n");
        return true;
    }
};
MoonRuntime runtime;
extern "C" uint32_t pokemon3ds_host_window_running() noexcept {
    return !runtime.nativeWindow || runtime.nativeWindow->IsRunning();
}
std::mutex mutex;
// Host-owned teardown runs before cross-library static logger destruction.
extern "C" uint32_t pokemon3ds_host_shutdown() noexcept {
    try {
        std::lock_guard guard(mutex);
        runtime.nativeBackend.reset();
        if (runtime.nativeWindow) {
            SDL_DelEventWatch(MoonSingleScreen::Event, nullptr);
            SDL_DestroyWindow(static_cast<SDL_Window*>(runtime.nativeWindow->GetNativeWindow()));
            runtime.nativeWindow.reset();
        }
        runtime.memoryContext = nullptr;
        runtime.guestRead = nullptr;
        runtime.backendReady = false;
        runtime.frameStarted = false;
        runtime.framePresented = false;
        std::fprintf(stderr, "NATIVE_HOST_RENDERER_SHUTDOWN=complete\n");
        return 1;
    } catch (...) {
        std::fprintf(stderr, "NATIVE_HOST_RENDERER_SHUTDOWN=failed\n");
        return 0;
    }
}
// Semantic snapshots only; caller must runtime-verify the mapping and root
// lifecycle before enabling. No numeric command ordering is assumed here.
extern "C" void moon_battle_root_state(uint32_t active,int selected,
                                       int fight,int bag,int pokemon,int run) noexcept {
    std::lock_guard<std::mutex> lock(mutex);
    runtime.battleCommandMap.ids={fight,bag,pokemon,run};
    runtime.battleSelected=selected;
    runtime.battleRootActive=active&&runtime.battleCommandMap.Verified();
    if(!runtime.battleRootActive)MoonSingleScreen::BattleClick=-1;
    for(unsigned i=0;i<4;i++)MoonSingleScreen::BattleCommands[i]=runtime.battleCommandMap.ids[i];
    MoonSingleScreen::BattleRootActive=runtime.battleRootActive;
}
// Guest-thread adapter consumes a native command request once. This does not
// itself dispatch a guest function from the renderer/event-loop thread.
extern "C" int moon_battle_root_click() noexcept {
    return MoonSingleScreen::BattleClick.exchange(-1);
}
extern "C" void moon_single_screen_context(uint32_t context) noexcept {
    if(MoonSingleScreen::Enabled()&&context>=1&&context<=5)MoonSingleScreen::Context=context;
}
extern "C" void moon_single_screen_command(uint32_t command) noexcept {
    if(!MoonSingleScreen::Enabled())return;
    if(command==9)MoonSingleScreen::DebugDual=!MoonSingleScreen::DebugDual.load();
    if(command==10)MoonSingleScreen::ManualPanel=!MoonSingleScreen::ManualPanel.load();
}
extern "C" void moon_single_screen_input(MoonSingleScreen::HostInput* input) noexcept {
    if(input)*input=MoonSingleScreen::Input();
}
// Optional additive ABI: the original 24-byte input snapshot stays unchanged.
extern "C" uint64_t moon_single_screen_press_edges() noexcept {
    return MoonSingleScreen::SamplePressEdges.load();
}
extern "C" uint32_t moon_single_screen_layout_enabled() noexcept {
    return MoonSingleScreen::Enabled() && !MoonSingleScreen::DebugDual.load() &&
        !MoonSingleScreen::ManualPanel.load();
}
bool MoonSink::ReadCommandList(uint32_t address, uint32_t size, std::vector<uint32_t>& words) {
    if (!runtime.guestRead || size > (1U << 20) || size % 4) return false;
    words.resize(size / 4);
    return runtime.guestRead(runtime.memoryContext, address, reinterpret_cast<uint8_t*>(words.data()), size);
}
// Callback context belongs to the Rust caller's stack, not the renderer.
// Invalidate it on every return, including exception paths.
struct GuestMemoryScope {
    GuestMemoryScope(void* context, GuestRead read) {
        runtime.memoryContext = context;
        runtime.guestRead = read;
    }
    ~GuestMemoryScope() {
        runtime.memoryContext = nullptr;
        runtime.guestRead = nullptr;
    }
};
} // namespace

extern "C" uint64_t moon_triaevum_submit_pica(
    uint32_t paddr, const uint8_t* bytes, uint32_t size,
    void* memoryContext, GuestRead guestRead) noexcept {
    if (bytes == nullptr || size == 0 || size % 4 != 0 || size > 1024 * 1024)
        return UINT64_MAX;
    try {
        std::lock_guard guard(mutex);
        GuestMemoryScope memoryScope(memoryContext, guestRead);
        runtime.borrowedReads.clear();
        std::vector<uint32_t> words(size / 4);
        std::memcpy(words.data(), bytes, size);
        Oot3dNativeGame::Oot3dGspCommandPacket command{};
        command.Control = 1;
        command.Parameters[0] = paddr;
        command.Parameters[1] = size;
        std::string error;
        Oot3dSourceRuntime::CtrGspCommand ctrCommand{command.Control, command.Parameters};
        if (!runtime.ctrBackend.SubmitCommand(ctrCommand, words))
            return UINT64_MAX;
        auto resolved = runtime.queue.TakePendingDraws();
        runtime.resolvedDraws += resolved.size();
        for (const auto& submission : resolved) {
            Oot3dNativeGame::Oot3dPicaVulkanDrawPlan plan;
            if (!Oot3dNativeGame::BuildOot3dPicaVulkanDrawPlan(
                    submission, plan, &error, &runtime.shaderCache)) {
                ++runtime.rejectedPlans;
                continue;
            }
            ++runtime.plannedDraws;
            if (const char* dump = std::getenv("POKEMOON_TRIAEVUM_SHADER_DUMP")) {
                if (runtime.dumpedPrograms.size() < 128 &&
                    runtime.dumpedPrograms.insert(plan.FragmentShader.StateKey).second) {
                    std::filesystem::create_directories(dump);
                    const auto base = std::string(dump) + "/draw-" + std::to_string(plan.SubmissionId);
                    const auto write = [&](const char* suffix, const void* data, size_t size) {
                        if (auto* file = std::fopen((base + suffix).c_str(), "wb")) {
                            std::fwrite(data, 1, size, file);
                            std::fclose(file);
                        }
                    };
                    const auto vertex = plan.ResolvedVertexShaderSource();
                    const auto fragment = plan.ResolvedFragmentShaderSource();
                    write(".vert", vertex.data(), vertex.size());
                    write(".frag", fragment.data(), fragment.size());
                    write(".regs", submission.Packet.Registers.data(), sizeof(submission.Packet.Registers));
                    write(".fragment-uniforms", &plan.FragmentShader.Uniforms, sizeof(plan.FragmentShader.Uniforms));
                    write(".vertex-uniforms", &plan.VertexShader.Uniforms, sizeof(plan.VertexShader.Uniforms));
                    for (const auto& texture : plan.Textures) {
                        const auto bytes = texture.ResolvedNativeBytes();
                        if (bytes.size() > (8U << 20)) continue;
                        const auto suffix = ".texture-" + std::to_string(texture.Slot);
                        write(suffix.c_str(), bytes.data(), bytes.size());
                        const auto stateSuffix = suffix + ".state";
                        write(stateSuffix.c_str(), &texture.State, sizeof(texture.State));
                    }
                }
            }
            runtime.vertexBuffers += plan.VertexBindings.size();
            runtime.indexBuffers += plan.Indexed && !plan.ResolvedIndexBytes().empty();
            runtime.textures += plan.Textures.size();
            std::vector<Fast::Renderer3ds::PicaVertexBindingView> bindings;
            std::vector<Fast::Renderer3ds::PicaVertexAttributeView> attributes;
            std::vector<Fast::Renderer3ds::PicaTextureView> textures;
            for (const auto& source : plan.VertexBindings)
                bindings.push_back({source.Binding, source.ByteStride,
                    source.InputRate == Oot3dNativeGame::Oot3dPicaVertexInputRate::PerInstance,
                    source.ResolvedBytes()});
            for (const auto& source : plan.VertexAttributes)
                attributes.push_back({source.Location, source.Binding,
                    static_cast<Fast::Renderer3ds::PicaVertexFormat>(source.Format),
                    source.ComponentCount, source.ByteOffset});
            for (const auto& source : plan.Textures) {
                const auto& texture = source.State;
                Fast::Renderer3ds::PicaTextureView view{};
                view.Slot = source.Slot;
                view.Width = texture.Width;
                view.Height = texture.Height;
                view.NativeFormat = texture.Format;
                view.NativeType = texture.Type;
                view.NativeWrapS = texture.WrapS;
                view.NativeWrapT = texture.WrapT;
                view.BorderRGBA = texture.BorderRGBA;
                view.MinLinear = texture.MinLinear;
                view.MagLinear = texture.MagLinear;
                view.MipLinear = texture.MipLinear;
                view.LodBiasRaw = texture.LodBiasRaw;
                view.MinMipLevel = texture.MinMipLevel;
                view.MaxMipLevel = texture.MaxMipLevel;
                view.PhysicalAddress = texture.PhysicalAddress;
                view.NativeBytes = source.ResolvedNativeBytes();
                textures.push_back(view);
            }
            Fast::Renderer3ds::PicaDrawView view{};
            view.CommandListAddress = plan.CommandListAddress;
            view.CommandListOffsetWords = plan.CommandListOffsetWords;
            view.VertexShaderSource = plan.ResolvedVertexShaderSource();
            view.FragmentShaderSource = plan.ResolvedFragmentShaderSource();
            view.VertexBindings = bindings;
            view.VertexAttributes = attributes;
            view.Textures = textures;
            view.IndexBytes = plan.ResolvedIndexBytes();
            view.Indexed = plan.Indexed;
            view.IndicesAre16Bit = plan.IndicesAre16Bit;
            view.BaseVertex = plan.BaseVertex;
            view.VertexCount = plan.VertexCount;
            view.FramebufferWidth = plan.State.Framebuffer.Width;
            view.FramebufferHeight = plan.State.Framebuffer.Height;
            // Bounded diagnostic: prove where native UI draws actually land.
            // A routed source queue/pass is not proof of render-target pixels.
            static unsigned uiTargetTraceCount=0;
            if(std::getenv("MOON_SINGLE_SCREEN_TRACE")&&
               MoonSingleScreen::Current()==MoonSingleScreen::NativePage&&
               uiTargetTraceCount++<192)
                std::fprintf(stderr,"MOON_UI_PICA_TARGET draw=%llu frame=%llu color=%08x size=%ux%u vertices=%u\n",
                    (unsigned long long)plan.SubmissionId,(unsigned long long)runtime.framesStarted,
                    plan.State.Framebuffer.ColorPhysicalAddress,plan.State.Framebuffer.Width,
                    plan.State.Framebuffer.Height,plan.VertexCount);
            view.FramebufferColorPhysicalAddress = plan.State.Framebuffer.ColorPhysicalAddress;
            view.FramebufferDepthPhysicalAddress = plan.State.Framebuffer.DepthPhysicalAddress;
            view.FramebufferColorFormat = plan.State.Framebuffer.ColorFormat;
            view.FramebufferDepthFormat = plan.State.Framebuffer.DepthFormat;
            if (!view.VertexShaderSource.empty() && !view.FragmentShaderSource.empty() &&
                !view.VertexBindings.empty() && !view.VertexAttributes.empty() &&
                view.VertexCount != 0) ++runtime.picaViews;
            if (runtime.EnsureBackend()) {
                ++runtime.submitAttempts;
                if (runtime.submitAttempts <= 4) std::fprintf(stderr, "MOON_DRAW id=%llu count=%u color=%08x depth=%08x size=%ux%u viewport=%.0fx%.0f\n", (unsigned long long)plan.SubmissionId, plan.VertexCount, plan.State.Framebuffer.ColorPhysicalAddress, plan.State.Framebuffer.DepthPhysicalAddress, plan.State.Framebuffer.Width, plan.State.Framebuffer.Height, plan.State.Viewport.HalfWidth * 2, plan.State.Viewport.HalfHeight * 2);
                std::string submitError;
                if (!runtime.frameStarted) {
                    runtime.nativeWindow->HandleEvents();
                    runtime.nativeBackend->StartFrame();
                    runtime.frameStarted = runtime.nativeBackend->HasActiveFrame();
                    if (!runtime.frameStarted) continue;
                    ++runtime.framesStarted;
                    for (const auto& fill : runtime.pendingFills) {
                        std::string fillError;
                        if (!runtime.nativeBackend->SubmitPicaMemoryFill(fill, &fillError))
                            std::fprintf(stderr, "TRIAEVUM_FILL_REJECTED: %s\n", fillError.c_str());
                    }
                    runtime.pendingFills.clear();
                }
                if (Oot3dNativeGame::SubmitOot3dPicaVulkanDrawPlan(*runtime.nativeBackend, plan, &submitError)) {
                    ++runtime.submitSuccess;
                    if (runtime.submitSuccess == 1) std::fprintf(stderr, "FIRST_TRIAEVUM_DRAW_SUBMITTED\n");
                } else {
                    ++runtime.submitRejected;
                    if (runtime.submitRejected == 1 || runtime.submitRejected % 1000 == 0)
                        std::fprintf(stderr, "TRIAEVUM_DRAW_REJECTED: %s\n", submitError.c_str());
                }
                if (runtime.submitAttempts == 1 || runtime.submitAttempts % 1000 == 0)
                    std::fprintf(stderr, "triaevum_submit_attempts=%llu success=%llu rejected=%llu\n", (unsigned long long)runtime.submitAttempts, (unsigned long long)runtime.submitSuccess, (unsigned long long)runtime.submitRejected);
            }
        }
        runtime.borrowedReads.clear();
        return runtime.sink.draws;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "TRIAEVUM_BRIDGE_ERROR: %s\n", error.what());
        return UINT64_MAX;
    } catch (...) {
        return UINT64_MAX;
    }
}

extern "C" uint64_t moon_triaevum_resolved_draws() noexcept {
    return runtime.resolvedDraws;
}

extern "C" uint64_t moon_triaevum_rejected_draws() noexcept {
    return runtime.sink.rejected;
}

extern "C" uint64_t moon_triaevum_pica_views() noexcept {
    return runtime.picaViews;
}

extern "C" uint64_t moon_triaevum_rejected_plans() noexcept {
    return runtime.rejectedPlans;
}

extern "C" uint64_t moon_triaevum_vertex_buffers() noexcept { return runtime.vertexBuffers; }
extern "C" uint64_t moon_triaevum_index_buffers() noexcept { return runtime.indexBuffers; }
extern "C" uint64_t moon_triaevum_textures() noexcept { return runtime.textures; }

extern "C" bool moon_triaevum_display_transfer(uint32_t input, uint32_t output, uint32_t inputSize, uint32_t outputSize, uint32_t flags, void* memoryContext, GuestRead guestRead) noexcept {
    try {
        std::lock_guard guard(mutex);
        GuestMemoryScope memoryScope(memoryContext, guestRead);
        if (!runtime.backendReady || !runtime.frameStarted) return false;
        Fast::Renderer3ds::PicaDisplayTransferView transfer{};
        transfer.CompletionId = ++runtime.transferCount;
        if (runtime.transferCount <= 4) std::fprintf(stderr, "MOON_TRANSFER src=%08x dst=%08x in=%08x out=%08x flags=%08x\n", input, output, inputSize, outputSize, flags);
        transfer.AfterDrawSubmissionId = runtime.resolvedDraws;
        // GX submits mapped VRAM addresses; PICA registers contain physical VRAM.
        transfer.InputPhysicalAddress = (input >= 0x1f000000 && input < 0x1f600000) ? input - 0x07000000 : input;
        transfer.OutputPhysicalAddress = (output >= 0x1f000000 && output < 0x1f600000)
            ? output - 0x07000000 : output;
        transfer.InputWidth = inputSize & 0xffff;
        transfer.InputHeight = inputSize >> 16;
        transfer.OutputWidth = outputSize & 0xffff;
        transfer.OutputHeight = outputSize >> 16;
        transfer.Flags = flags;
        transfer.Present = false;
        std::string error;
        // Production API separates transfer recording from snapshot scanout.
        bool accepted = runtime.nativeBackend->SubmitPicaDisplayTransfer(transfer, &error);
        if (accepted) {
            if (MoonSingleScreen::Enabled()) {
                const auto mode=MoonSingleScreen::Current();
                const bool top=transfer.OutputHeight==400;
                transfer.Present=top || (mode!=MoonSingleScreen::NormalSingleScreen&&mode!=MoonSingleScreen::NativePage&&mode!=MoonSingleScreen::BattleRoot);
                if(top) transfer.PresentationRegion=mode==MoonSingleScreen::DualScreen
                    ?Fast::Renderer3ds::ScreenRegion{0,0,1,.5f}:Fast::Renderer3ds::ScreenRegion{};
                else {transfer.PresentationRegion=MoonSingleScreen::BottomRegion();transfer.PresentationMode=Fast::Renderer3ds::PicaPresentationMode::AlphaOverlay;transfer.PresentationOpaque=mode!=MoonSingleScreen::DualScreen;}
                static uint32_t previous=UINT32_MAX;
                if(previous!=mode){previous=mode;std::fprintf(stderr,"SINGLE_SCREEN_PRESENT mode=%u top=400x240 bottom=320x240\n",mode);}
            } else if (!std::getenv("POKEMOON_TRIAEVUM_CAPTURE_TOP_ONLY") || transfer.OutputHeight == 400) {
                transfer.Present = true;
#ifdef __ANDROID__
                // Original screen semantics on Android, not the shelved desktop overlay.
                const bool top=transfer.OutputHeight==400;
                transfer.PresentationRegion=top?Fast::Renderer3ds::ScreenRegion{0,0,1,.5f}
                    :Fast::Renderer3ds::ScreenRegion{0,.5f,1,.5f};
                if(!top)transfer.PresentationMode=Fast::Renderer3ds::PicaPresentationMode::AlphaOverlay;
#else
                if (std::getenv("TRIAEVUM_DUAL_SCREEN_SCANOUT") && transfer.OutputHeight == 320)
                    transfer.PresentationMode = Fast::Renderer3ds::PicaPresentationMode::AlphaOverlay;
#endif
            }
        }
        if (accepted && transfer.Present)
            accepted = runtime.nativeBackend->SubmitPicaDisplayTransfer(transfer, &error);
        runtime.framePresented |= accepted && transfer.Present;
        if (accepted && (MoonSingleScreen::Enabled() || std::getenv("TRIAEVUM_DUAL_SCREEN_SCANOUT")) && transfer.OutputHeight == 400)
            return true;
        runtime.DrawBattleRoot();
        runtime.nativeBackend->EndFrame();
        ++runtime.framesCompleted;
        runtime.nativeBackend->FinishRender();
        if (runtime.framePresented) ++runtime.framesPresented;
        runtime.framePresented = false;
        if (runtime.transferCount == 1 || runtime.transferCount % 600 == 0)
            std::fprintf(stderr, "triaevum_frames_started=%llu completed=%llu presented=%llu submit_success=%llu\n", (unsigned long long)runtime.framesStarted, (unsigned long long)runtime.framesCompleted, (unsigned long long)runtime.framesPresented, (unsigned long long)runtime.submitSuccess);
        runtime.frameStarted = false;
        if (std::getenv("POKEMOON_TRIAEVUM_DUMP") && runtime.transferCount == 60) {
            Fast::Renderer3ds::PicaPresentationStateSnapshot presentation;
            if (runtime.nativeBackend->CapturePicaPresentationState(presentation, &error)) {
                for (const auto& snapshot : presentation.DisplayImages) {
                    char path[128];
                    std::snprintf(path, sizeof(path), "/tmp/moon-triaevum-display-%08x.ppm", snapshot.OutputPhysicalAddress);
                    if (auto* file = std::fopen(path, "wb")) {
                        std::fprintf(file, "P6\n%u %u\n255\n", snapshot.ImageWidth, snapshot.ImageHeight);
                        for (size_t pixel = 0; pixel + 3 < snapshot.ColorRgba8.size(); pixel += 4)
                            std::fwrite(snapshot.ColorRgba8.data() + pixel, 1, 3, file);
                        std::fclose(file);
                    }
                }
            }
            std::vector<Fast::Renderer3ds::PicaRenderTargetColorSnapshot> snapshots;
            if (runtime.nativeBackend->CapturePicaColorTargets(snapshots, &error)) {
                for (const auto& snapshot : snapshots) {
                    char path[128];
                    std::snprintf(path, sizeof(path), "/tmp/moon-triaevum-target-%08x.ppm", snapshot.ColorPhysicalAddress);
                    if (auto* file = std::fopen(path, "wb")) {
                        std::fprintf(file, "P6\n%u %u\n255\n", snapshot.ImageWidth, snapshot.ImageHeight);
                        for (size_t pixel = 0; pixel + 3 < snapshot.ColorRgba8.size(); pixel += 4)
                            std::fwrite(snapshot.ColorRgba8.data() + pixel, 1, 3, file);
                        std::fclose(file);
                    }
                }
            } else std::fprintf(stderr, "TRIAEVUM_CAPTURE_ERROR: %s\n", error.c_str());
        }
        if (!accepted) std::fprintf(stderr, "TRIAEVUM_TRANSFER_REJECTED: %s\n", error.c_str());
        return accepted;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "TRIAEVUM_PRESENT_ERROR: %s\n", error.what());
        runtime.frameStarted = runtime.nativeBackend && runtime.nativeBackend->HasActiveFrame();
        return false;
    } catch (...) { return false; }
}

extern "C" void moon_triaevum_memory_fill(uint32_t start, uint32_t end, uint32_t value, uint32_t width) noexcept {
    try {
        std::lock_guard guard(mutex);
        Fast::Renderer3ds::PicaMemoryFillView fill{};
        // GX fill commands use the same mapped VRAM range as transfers.
        // Render-target identities are physical; otherwise depth is never cleared.
        fill.StartPhysicalAddress = (start >= 0x1f000000 && start < 0x1f600000)
            ? start - 0x07000000 : start;
        fill.EndPhysicalAddress = (end >= 0x1f000000 && end <= 0x1f600000)
            ? end - 0x07000000 : end;
        fill.Value = value;
        fill.Control = 1 | (width == 3 ? 1 << 8 : width == 2 ? 0 : 1 << 9);
        if (runtime.frameStarted) {
            std::string error;
            runtime.nativeBackend->SubmitPicaMemoryFill(fill, &error);
        } else if (runtime.pendingFills.size() < 128) {
            runtime.pendingFills.push_back(fill);
        }
    } catch (...) {}
}

#ifdef __ANDROID__
extern "C" uint32_t moon_triaevum_android_map_touch(float x,float y,float width,float height) noexcept {
    if(!std::isfinite(x)||!std::isfinite(y)||!std::isfinite(width)||!std::isfinite(height))return 0;
    const auto rectangle=Fast::Renderer3ds::FitScreenSurface(width,height,320,240,{0,.5f,1,.5f});
    const auto point=Fast::Renderer3ds::WindowToSurface(x,y,rectangle,320,240);
    if(!point)return 0;
    return 0x80000000u|(std::min(239u,static_cast<uint32_t>(point->Y))<<16)
        |std::min(319u,static_cast<uint32_t>(point->X));
}
extern "C" void moon_triaevum_android_release_surface() noexcept {
    try {
        std::lock_guard guard(mutex);
        runtime.nativeBackend.reset();
        if(runtime.nativeWindow){
            SDL_DelEventWatch(MoonSingleScreen::Event,nullptr);
            SDL_DestroyWindow(static_cast<SDL_Window*>(runtime.nativeWindow->GetNativeWindow()));
            runtime.nativeWindow.reset();
        }
        runtime.backendReady=false;runtime.frameStarted=false;runtime.framePresented=false;
        std::fprintf(stderr,"TRIAEVUM_ANDROID_SURFACE_RELEASED\n");
    } catch (...) {std::fprintf(stderr,"TRIAEVUM_ANDROID_SURFACE_RELEASE_FAILED\n");}
}
// SDLActivity can launch another title session in the same process. Guest
// execution has stopped before this call; no borrowed memory survives it.
extern "C" void moon_triaevum_android_shutdown() noexcept {
    try {
        std::lock_guard guard(mutex);
        SDL_DelEventWatch(MoonSingleScreen::Event,nullptr);
        std::destroy_at(&runtime);
        std::construct_at(&runtime);
    } catch (...) { std::fprintf(stderr,"TRIAEVUM_ANDROID_SHUTDOWN_FAILED\n"); }
}
#endif
