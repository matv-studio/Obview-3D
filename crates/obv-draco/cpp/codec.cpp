// Encodage et décodage Draco par la bibliothèque de référence (D-30).
#include "codec.h"

#include <draco/compression/decode.h>
#include <draco/compression/expert_encode.h>
#include <draco/core/draco_types.h>
#include <draco/mesh/mesh.h>

#include "obv-draco/src/ffi.rs.h"

namespace obv_draco {

namespace {

// Ajoute un attribut flottant à `mesh` (une valeur par point, correspondance identique).
int add_attribute(draco::Mesh& mesh, draco::GeometryAttribute::Type type, const float* values, int components,
                  uint32_t points) {
  draco::GeometryAttribute ga;
  ga.Init(type, nullptr, static_cast<uint8_t>(components), draco::DT_FLOAT32, false,
          sizeof(float) * static_cast<int64_t>(components), 0);
  const int id = mesh.AddAttribute(ga, true, points);
  draco::PointAttribute* attr = mesh.attribute(id);
  for (uint32_t i = 0; i < points; ++i) {
    attr->SetAttributeValue(draco::AttributeValueIndex(i), values + static_cast<size_t>(i) * components);
  }
  return id;
}

} // namespace

Encoded encode(rust::Slice<const float> positions, rust::Slice<const float> normals, rust::Slice<const float> uvs,
               rust::Slice<const uint32_t> indices, const EncodeOptions& options) {
  Encoded out{};
  out.position_id = out.normal_id = out.uv_id = -1;
  const uint32_t points = static_cast<uint32_t>(positions.size() / 3);
  if (points == 0 || indices.size() < 3) {
    out.error = "maillage vide";
    return out;
  }
  draco::Mesh mesh;
  mesh.set_num_points(points);
  const int pos = add_attribute(mesh, draco::GeometryAttribute::POSITION, positions.data(), 3, points);
  int nrm = -1, uv = -1;
  if (normals.size() == static_cast<size_t>(points) * 3) {
    nrm = add_attribute(mesh, draco::GeometryAttribute::NORMAL, normals.data(), 3, points);
  }
  if (uvs.size() == static_cast<size_t>(points) * 2) {
    uv = add_attribute(mesh, draco::GeometryAttribute::TEX_COORD, uvs.data(), 2, points);
  }
  const size_t faces = indices.size() / 3;
  mesh.SetNumFaces(faces);
  for (size_t f = 0; f < faces; ++f) {
    draco::Mesh::Face face;
    for (int c = 0; c < 3; ++c) {
      const uint32_t v = indices[f * 3 + c];
      if (v >= points) {
        out.error = "indice hors limites";
        return out;
      }
      face[c] = draco::PointIndex(v);
    }
    mesh.SetFace(draco::FaceIndex(static_cast<uint32_t>(f)), face);
  }

  draco::ExpertEncoder encoder(mesh);
  encoder.SetSpeedOptions(options.encode_speed, options.decode_speed);
  encoder.SetAttributeQuantization(pos, options.position_bits);
  if (nrm >= 0) {
    encoder.SetAttributeQuantization(nrm, options.normal_bits);
  }
  if (uv >= 0) {
    encoder.SetAttributeQuantization(uv, options.uv_bits);
  }
  // Comptes des points et faces encodés : ce sont ceux des accesseurs glTF (Edgebreaker peut dédoubler des
  // sommets non variétés).
  encoder.SetTrackEncodedProperties(true);
  draco::EncoderBuffer buffer;
  const draco::Status status = encoder.EncodeToBuffer(&buffer);
  if (!status.ok()) {
    out.error = rust::String(status.error_msg_string());
    return out;
  }
  out.data.reserve(buffer.size());
  const auto* bytes = reinterpret_cast<const uint8_t*>(buffer.data());
  for (size_t i = 0; i < buffer.size(); ++i) {
    out.data.push_back(bytes[i]);
  }
  out.points = static_cast<uint32_t>(encoder.num_encoded_points());
  out.faces = static_cast<uint32_t>(encoder.num_encoded_faces());
  out.position_id = static_cast<int32_t>(mesh.attribute(pos)->unique_id());
  out.normal_id = nrm >= 0 ? static_cast<int32_t>(mesh.attribute(nrm)->unique_id()) : -1;
  out.uv_id = uv >= 0 ? static_cast<int32_t>(mesh.attribute(uv)->unique_id()) : -1;
  return out;
}

Decoded::Decoded(std::unique_ptr<draco::Mesh> mesh, std::string error)
    : mesh_(std::move(mesh)), error_(std::move(error)) {}

Decoded::~Decoded() = default;

rust::String Decoded::decode_error() const { return rust::String(error_); }

uint32_t Decoded::num_points() const { return mesh_ ? mesh_->num_points() : 0; }

rust::Vec<uint32_t> Decoded::indices() const {
  rust::Vec<uint32_t> out;
  if (!mesh_) {
    return out;
  }
  out.reserve(static_cast<size_t>(mesh_->num_faces()) * 3);
  for (draco::FaceIndex f(0); f < mesh_->num_faces(); ++f) {
    const auto& face = mesh_->face(f);
    for (int c = 0; c < 3; ++c) {
      out.push_back(face[c].value());
    }
  }
  return out;
}

rust::Vec<float> Decoded::attribute(uint32_t id, uint32_t components) const {
  rust::Vec<float> out;
  if (!mesh_) {
    return out;
  }
  const draco::PointAttribute* attr = mesh_->GetAttributeByUniqueId(id);
  if (!attr || components == 0 || components > 4) {
    return out;
  }
  out.reserve(static_cast<size_t>(mesh_->num_points()) * components);
  float value[4] = {0, 0, 0, 0};
  for (draco::PointIndex p(0); p < mesh_->num_points(); ++p) {
    // ConvertValue applique la normalisation des entiers quand l'attribut est normalisé.
    attr->ConvertValue<float>(attr->mapped_index(p), static_cast<int8_t>(components), value);
    for (uint32_t c = 0; c < components; ++c) {
      out.push_back(value[c]);
    }
  }
  return out;
}

std::unique_ptr<Decoded> decode(rust::Slice<const uint8_t> data) {
  draco::DecoderBuffer buffer;
  buffer.Init(reinterpret_cast<const char*>(data.data()), data.size());
  draco::Decoder decoder;
  auto result = decoder.DecodeMeshFromBuffer(&buffer);
  if (!result.ok()) {
    return std::make_unique<Decoded>(nullptr, result.status().error_msg_string());
  }
  return std::make_unique<Decoded>(std::move(result).value(), std::string());
}

} // namespace obv_draco
