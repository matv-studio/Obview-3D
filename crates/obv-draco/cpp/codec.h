#pragma once
#include <cstdint>
#include <memory>

#include "rust/cxx.h"

namespace draco {
class Mesh;
}

namespace obv_draco {

struct Encoded;
struct EncodeOptions;

Encoded encode(rust::Slice<const float> positions, rust::Slice<const float> normals, rust::Slice<const float> uvs,
               rust::Slice<const uint32_t> indices, const EncodeOptions& options);

class Decoded {
public:
  Decoded(std::unique_ptr<draco::Mesh> mesh, std::string error);
  ~Decoded();
  rust::String decode_error() const;
  uint32_t num_points() const;
  rust::Vec<uint32_t> indices() const;
  rust::Vec<float> attribute(uint32_t id, uint32_t components) const;

private:
  std::unique_ptr<draco::Mesh> mesh_;
  std::string error_;
};

std::unique_ptr<Decoded> decode(rust::Slice<const uint8_t> data);

} // namespace obv_draco
