// Local diagnostic: generic TriAevum PICA texture decoding, no game semantics.
#include "oot3d/renderer/pica_texture_decode.h"
#include <fstream>
#include <iterator>
#include <iostream>
int main(int argc, char** argv) {
    if (argc != 6) return 2;
    std::ifstream in(argv[1], std::ios::binary);
    std::vector<uint8_t> bytes{std::istreambuf_iterator<char>(in), {}};
    std::vector<uint8_t> pixels;
    std::string error;
    const int width = std::stoi(argv[2]), height = std::stoi(argv[3]);
    if (!Oot3d::Renderer::DecodePicaTextureRgba8(std::stoi(argv[4]), width,
            height, bytes, pixels, &error)) {
        std::cerr << error << '\n'; return 1;
    }
    std::ofstream out(argv[5], std::ios::binary);
    out << "P6\n" << width << ' ' << height << "\n255\n";
    for (size_t i = 0; i < pixels.size(); i += 4)
        out.write(reinterpret_cast<const char*>(pixels.data() + i), 3);
}
