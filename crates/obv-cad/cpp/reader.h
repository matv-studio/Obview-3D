// Lecteur STEP / IGES d'Obview 3D sur OpenCASCADE (XCAF) : assemblages, noms, couleurs, maillage (D-27).
#pragma once

#include "rust/cxx.h"

namespace obv {
struct CadOptions;
struct CadResult;

CadResult read_cad(rust::Str path, bool iges, const CadOptions& options);
bool write_test_step(rust::Str path);
} // namespace obv
