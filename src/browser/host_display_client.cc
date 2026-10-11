#include "carbonyl/src/browser/host_display_client.h"

#include <utility>
#include "base/numerics/checked_math.h"

#include "mojo/public/cpp/system/platform_handle.h"
#include "skia/ext/platform_canvas.h"
#include "third_party/skia/include/core/SkColor.h"
#include "third_party/skia/include/core/SkRect.h"
#include "third_party/skia/src/core/SkDevice.h"
#include "ui/gfx/skia_util.h"

#if BUILDFLAG(IS_WIN)
#include "skia/ext/skia_utils_win.h"
#endif

#include "carbonyl/src/browser/renderer.h"

namespace carbonyl {

LayeredWindowUpdater::LayeredWindowUpdater(
    mojo::PendingReceiver<viz::mojom::LayeredWindowUpdater> receiver, gfx::AcceleratedWidget widget
)
  :
    widget_(widget),
    receiver_(this, std::move(receiver)),
    task_runner_(base::SingleThreadTaskRunner::GetCurrentDefault())
  {}

LayeredWindowUpdater::~LayeredWindowUpdater() = default;

void LayeredWindowUpdater::OnAllocatedSharedMemory(
    const gfx::Size& pixel_size,
    base::UnsafeSharedMemoryRegion region) {
  shm_mapping_ = std::make_shared<base::WritableSharedMemoryMapping>(
      region.IsValid() ? region.Map() : base::WritableSharedMemoryMapping());

  pixel_size_ = pixel_size;
}

void LayeredWindowUpdater::Draw(const gfx::Rect& damage_rect,
                                DrawCallback callback) {
  size_t required_bytes = 0;
  auto bytes = base::CheckedNumeric<size_t>(pixel_size_.width()) * pixel_size_.height() * 4;
  if (!shm_mapping_ || !shm_mapping_->IsValid() || !bytes.AssignIfValid(&required_bytes) ||
      shm_mapping_->size() < required_bytes) {
    std::move(callback).Run();
    return;
  }
  auto* renderer=Renderer::ForWidget(widget_);
  if (!renderer && !std::getenv("CARBONYL_SESSION_SOCKET")) renderer=Renderer::GetCurrent();
  if (!renderer) { std::move(callback).Run(); return; }
  renderer->DrawBitmap(
    shm_mapping_->GetMemoryAs<uint8_t>(),
    pixel_size_,
    damage_rect,
    base::BindOnce(
      [](
        scoped_refptr<base::SingleThreadTaskRunner> task_runner,
        std::shared_ptr<base::WritableSharedMemoryMapping> mapping,
        DrawCallback callback
      ) {
        task_runner->PostTask(FROM_HERE, std::move(callback));
      },
      task_runner_,
      shm_mapping_,  // Keep this frame alive if the next resize replaces its buffer.
      std::move(callback)
    )
  );
}

HostDisplayClient::HostDisplayClient(gfx::AcceleratedWidget widget)
    : viz::HostDisplayClient(widget), widget_(widget) {}
HostDisplayClient::~HostDisplayClient() = default;

void HostDisplayClient::CreateLayeredWindowUpdater(
    mojo::PendingReceiver<viz::mojom::LayeredWindowUpdater> receiver) {
  layered_window_updater_ =
      std::make_unique<LayeredWindowUpdater>(std::move(receiver), widget_);
}

#if BUILDFLAG(IS_MAC)
void HostDisplayClient::OnDisplayReceivedCALayerParams(
    const gfx::CALayerParams& ca_layer_params) {}
#endif

#if BUILDFLAG(IS_LINUX) && BUILDFLAG(SUPPORTS_OZONE_X11)
void HostDisplayClient::DidCompleteSwapWithNewSize(
    const gfx::Size& size) {}
#endif

}  // namespace carbonyl
