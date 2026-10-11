#ifndef CARBONYL_SRC_BROWSER_NATIVE_TEXT_H_
#define CARBONYL_SRC_BROWSER_NATIVE_TEXT_H_

#include <string_view>

namespace carbonyl {
// Paint-record metadata identifies DOM text sent to the terminal. Raster text
// from Canvas and other drawing APIs must remain ordinary, drawable glyphs.
inline constexpr char kNativeTextMarkerBytes[] = "\0carbonyl-native-text:";
inline constexpr std::string_view kNativeTextMarker{
    kNativeTextMarkerBytes, sizeof(kNativeTextMarkerBytes) - 1};
}  // namespace carbonyl

#endif  // CARBONYL_SRC_BROWSER_NATIVE_TEXT_H_
