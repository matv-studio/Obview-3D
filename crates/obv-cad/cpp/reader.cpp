// Lecteur STEP / IGES d'Obview 3D sur OpenCASCADE (XCAF) : assemblages, noms, couleurs, maillage (D-27).
//
// Le document XCAF est parcouru feuille par feuille (XCAFPrs_DocumentExplorer : instances avec leur position
// globale et leur style hérité). Chaque forme de référence est maillée une seule fois (BRepMesh, en
// parallèle), puis chaque instance reçoit sa copie transformée. Les couleurs par face sont lues par
// XCAFPrs::CollectStyleSettings ; les triangles d'une pièce sont regroupés par style.

#include "obv-cad/cpp/reader.h"
#include "obv-cad/src/ffi.rs.h"

#include <BRepBndLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <STEPControl_StepModelType.hxx>
#include <Quantity_Color.hxx>
#include <Quantity_ColorRGBA.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <IGESCAFControl_Reader.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <Message_ProgressRange.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <Standard_Failure.hxx>
#include <TDF_Label.hxx>
#include <TDF_LabelSequence.hxx>
#include <TDF_Tool.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <XCAFPrs.hxx>
#include <XCAFPrs_DocumentExplorer.hxx>
#include <XCAFPrs_Style.hxx>
#include <UnitsMethods_LengthUnit.hxx>

#include <algorithm>
#include <cmath>
#include <map>
#include <string>
#include <vector>

namespace obv {
namespace {

// Nom d'un label (attribut TDataStd_Name), en UTF-8.
std::string label_name(const TDF_Label& label) {
  occ::handle<TDataStd_Name> name;
  if (label.IsNull() || !label.FindAttribute(TDataStd_Name::GetID(), name)) {
    return {};
  }
  const TCollection_ExtendedString& s = name->Get();
  std::string out;
  out.reserve(s.Length());
  for (int i = 1; i <= s.Length(); ++i) {
    const char32_t c = s.Value(i);
    if (c < 0x80) {
      out.push_back(static_cast<char>(c));
    } else if (c < 0x800) {
      out.push_back(static_cast<char>(0xC0 | (c >> 6)));
      out.push_back(static_cast<char>(0x80 | (c & 0x3F)));
    } else {
      out.push_back(static_cast<char>(0xE0 | (c >> 12)));
      out.push_back(static_cast<char>(0x80 | ((c >> 6) & 0x3F)));
      out.push_back(static_cast<char>(0x80 | (c & 0x3F)));
    }
  }
  return out;
}

// Maillage d'une forme de référence (repère propre), regroupé par style.
struct Group {
  uint32_t first = 0;
  uint32_t count = 0;
  XCAFPrs_Style style;
};
struct Prototype {
  std::vector<float> positions;
  std::vector<float> normals;
  std::vector<uint32_t> indices;
  std::vector<Group> groups;
};

// Les styles gardés sont ceux de la forme de référence : le style de l'instance s'applique à la copie.
Prototype mesh_prototype(const TDF_Label& ref, const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, XCAFPrs_Style, TopTools_ShapeMapHasher> styles;
  XCAFPrs::CollectStyleSettings(ref, TopLoc_Location(), styles);
  // Style de chaque face : le sien, sinon celui d'un sous-ensemble qui la contient, sinon celui hérité.
  std::vector<std::pair<TopoDS_Face, XCAFPrs_Style>> faces;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    XCAFPrs_Style st;
    for (int i = 1; i <= styles.Extent(); ++i) {
      const TopoDS_Shape& sub = styles.FindKey(i);
      bool contains = sub.IsSame(face);
      if (!contains && sub.ShapeType() < TopAbs_FACE) {
        for (TopExp_Explorer e2(sub, TopAbs_FACE); e2.More(); e2.Next()) {
          if (e2.Current().IsSame(face)) {
            contains = true;
            break;
          }
        }
      }
      if (contains) {
        const XCAFPrs_Style& s = styles.FindFromIndex(i);
        if (s.IsSetColorSurf()) {
          st.SetColorSurf(s.GetColorSurfRGBA());
        }
        if (!s.IsVisible()) {
          st.SetVisibility(false);
        }
        if (sub.IsSame(face)) {
          break;
        }
      }
    }
    faces.emplace_back(face, st);
  }
  // Regroupement par style (ordre de première apparition), puis triangles face par face.
  std::vector<XCAFPrs_Style> order;
  std::vector<int> face_group(faces.size());
  for (size_t f = 0; f < faces.size(); ++f) {
    int g = -1;
    for (size_t k = 0; k < order.size(); ++k) {
      if (order[k].IsEqual(faces[f].second)) {
        g = static_cast<int>(k);
        break;
      }
    }
    if (g < 0) {
      g = static_cast<int>(order.size());
      order.push_back(faces[f].second);
    }
    face_group[f] = g;
  }
  Prototype out;
  for (size_t g = 0; g < order.size(); ++g) {
    if (!order[g].IsVisible()) {
      continue;
    }
    Group grp;
    grp.first = static_cast<uint32_t>(out.indices.size() / 3);
    grp.style = order[g];
    for (size_t f = 0; f < faces.size(); ++f) {
      if (face_group[f] != static_cast<int>(g)) {
        continue;
      }
      const TopoDS_Face& face = faces[f].first;
      TopLoc_Location loc;
      occ::handle<Poly_Triangulation> tri = BRep_Tool::Triangulation(face, loc);
      if (tri.IsNull() || tri->NbTriangles() == 0) {
        continue;
      }
      if (!tri->HasNormals()) {
        BRepLib_ToolTriangulatedShape::ComputeNormals(face, tri);
      }
      const gp_Trsf t = loc.Transformation();
      const bool reversed = face.Orientation() == TopAbs_REVERSED;
      const uint32_t base = static_cast<uint32_t>(out.positions.size() / 3);
      for (int i = 1; i <= tri->NbNodes(); ++i) {
        gp_Pnt p = tri->Node(i).Transformed(t);
        out.positions.push_back(static_cast<float>(p.X()));
        out.positions.push_back(static_cast<float>(p.Y()));
        out.positions.push_back(static_cast<float>(p.Z()));
        gp_Dir n = tri->HasNormals() ? gp_Dir(tri->Normal(i)) : gp_Dir(0, 0, 1);
        n.Transform(t);
        if (reversed) {
          n.Reverse();
        }
        out.normals.push_back(static_cast<float>(n.X()));
        out.normals.push_back(static_cast<float>(n.Y()));
        out.normals.push_back(static_cast<float>(n.Z()));
      }
      for (int i = 1; i <= tri->NbTriangles(); ++i) {
        int a, b, c;
        tri->Triangle(i).Get(a, b, c);
        if (reversed) {
          std::swap(b, c);
        }
        out.indices.push_back(base + a - 1);
        out.indices.push_back(base + b - 1);
        out.indices.push_back(base + c - 1);
      }
    }
    grp.count = static_cast<uint32_t>(out.indices.size() / 3) - grp.first;
    if (grp.count > 0) {
      out.groups.push_back(grp);
    }
  }
  return out;
}

} // namespace

CadResult read_cad(rust::Str path, bool iges, const CadOptions& options) {
  CadResult result;
  result.unit_m = 0.001;
  try {
    // Pas de bavardage d'OpenCASCADE sur la sortie standard.
    Message::DefaultMessenger()->RemovePrinters(STANDARD_TYPE(Message_PrinterOStream));
    const std::string file(path.data(), path.size());
    occ::handle<XCAFApp_Application> app = XCAFApp_Application::GetApplication();
    occ::handle<TDocStd_Document> doc;
    app->NewDocument("BinXCAF", doc);
    bool ok = false;
    if (iges) {
      IGESCAFControl_Reader reader;
      reader.SetColorMode(true);
      reader.SetNameMode(true);
      reader.SetLayerMode(true);
      ok = reader.ReadFile(file.c_str()) == IFSelect_RetDone && reader.Transfer(doc);
    } else {
      STEPCAFControl_Reader reader;
      reader.SetColorMode(true);
      reader.SetNameMode(true);
      reader.SetLayerMode(true);
      reader.SetPropsMode(true);
      ok = reader.ReadFile(file.c_str()) == IFSelect_RetDone && reader.Transfer(doc);
    }
    if (!ok) {
      result.error = rust::String("lecture refusée par OpenCASCADE (fichier illisible ou incomplet)");
      return result;
    }
    double unit = 0.0;
    if (XCAFDoc_DocumentTool::GetLengthUnit(doc, unit, UnitsMethods_LengthUnit_Meter) && unit > 0.0) {
      result.unit_m = unit;
    }

    // Flèche absolue tirée de la diagonale de tout le modèle.
    occ::handle<XCAFDoc_ShapeTool> shapes = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
    TDF_LabelSequence roots;
    shapes->GetFreeShapes(roots);
    Bnd_Box box;
    for (int i = 1; i <= roots.Length(); ++i) {
      BRepBndLib::Add(XCAFDoc_ShapeTool::GetShape(roots.Value(i)), box);
    }
    double diag = box.IsVoid() ? 1.0 : std::sqrt(box.SquareExtent());
    const double deflection = std::max(diag * options.linear, 1e-7);

    std::map<std::string, Prototype> cache;
    for (XCAFPrs_DocumentExplorer ex(doc, XCAFPrs_DocumentExplorerFlags_OnlyLeafNodes); ex.More(); ex.Next()) {
      const XCAFPrs_DocumentNode& node = ex.Current();
      if (!node.Style.IsVisible()) {
        continue;
      }
      TCollection_AsciiString entry;
      TDF_Tool::Entry(node.RefLabel, entry);
      const std::string key(entry.ToCString());
      auto it = cache.find(key);
      if (it == cache.end()) {
        TopoDS_Shape shape = XCAFDoc_ShapeTool::GetShape(node.RefLabel);
        if (shape.IsNull()) {
          continue;
        }
        BRepMesh_IncrementalMesh mesher(shape, deflection, false, options.angular, true);
        it = cache.emplace(key, mesh_prototype(node.RefLabel, shape)).first;
      }
      const Prototype& proto = it->second;
      if (proto.indices.empty()) {
        continue;
      }
      CadPart part;
      // Nom de l'instance, sinon du produit (OCCT nomme « =>[0:1:1:n] » les instances sans nom).
      std::string name = label_name(node.Label);
      if (name.empty() || name.rfind("=>", 0) == 0) {
        name = label_name(node.RefLabel);
      }
      part.name = rust::String(name);
      std::string chain;
      for (int d = 0; d < ex.CurrentDepth(); ++d) {
        std::string n = label_name(ex.Current(d).Label);
        if (n.rfind("=>", 0) == 0) {
          n = label_name(ex.Current(d).RefLabel);
        }
        if (!n.empty()) {
          chain += chain.empty() ? n : " / " + n;
        }
      }
      part.path = rust::String(chain);
      const gp_Trsf t = node.Location.Transformation();
      part.positions.reserve(proto.positions.size());
      part.normals.reserve(proto.normals.size());
      for (size_t i = 0; i + 2 < proto.positions.size(); i += 3) {
        gp_Pnt p(proto.positions[i], proto.positions[i + 1], proto.positions[i + 2]);
        p.Transform(t);
        part.positions.push_back(static_cast<float>(p.X()));
        part.positions.push_back(static_cast<float>(p.Y()));
        part.positions.push_back(static_cast<float>(p.Z()));
        gp_Vec n(proto.normals[i], proto.normals[i + 1], proto.normals[i + 2]);
        n.Transform(t);
        const double l = n.Magnitude();
        if (l > 0.0) {
          n /= l;
        }
        part.normals.push_back(static_cast<float>(n.X()));
        part.normals.push_back(static_cast<float>(n.Y()));
        part.normals.push_back(static_cast<float>(n.Z()));
      }
      // Transformation miroir : l'ordre des sommets s'inverse pour garder les faces vers l'extérieur.
      const bool mirrored = t.VectorialPart().Determinant() < 0.0;
      part.indices.reserve(proto.indices.size());
      for (size_t i = 0; i + 2 < proto.indices.size(); i += 3) {
        part.indices.push_back(proto.indices[i]);
        part.indices.push_back(proto.indices[mirrored ? i + 2 : i + 1]);
        part.indices.push_back(proto.indices[mirrored ? i + 1 : i + 2]);
      }
      for (const Group& g : proto.groups) {
        CadGroup cg;
        cg.first = g.first;
        cg.count = g.count;
        // Couleur propre à la face, sinon celle de l'instance (héritée de l'assemblage).
        const XCAFPrs_Style& own = g.style;
        cg.has_color = own.IsSetColorSurf() || node.Style.IsSetColorSurf();
        if (cg.has_color) {
          const Quantity_ColorRGBA& c = own.IsSetColorSurf() ? own.GetColorSurfRGBA() : node.Style.GetColorSurfRGBA();
          double r = 0.0, gg = 0.0, b = 0.0;
          c.GetRGB().Values(r, gg, b, Quantity_TOC_RGB);
          cg.color = {static_cast<float>(r), static_cast<float>(gg), static_cast<float>(b), c.Alpha()};
        } else {
          cg.color = {0.0f, 0.0f, 0.0f, 1.0f};
        }
        part.groups.push_back(cg);
      }
      result.parts.push_back(std::move(part));
    }
  } catch (const Standard_Failure& e) {
    result.error = rust::String(std::string("OpenCASCADE : ") + (e.GetMessageString() ? e.GetMessageString() : "erreur"));
  } catch (const std::exception& e) {
    result.error = rust::String(std::string("lecture : ") + e.what());
  }
  return result;
}

bool write_test_step(rust::Str path) {
  try {
    occ::handle<XCAFApp_Application> app = XCAFApp_Application::GetApplication();
    occ::handle<TDocStd_Document> doc;
    app->NewDocument("BinXCAF", doc);
    occ::handle<XCAFDoc_ShapeTool> shapes = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
    occ::handle<XCAFDoc_ColorTool> colors = XCAFDoc_DocumentTool::ColorTool(doc->Main());
    TopoDS_Shape cube = BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape();
    TDF_Label proto = shapes->AddShape(cube, false);
    TDataStd_Name::Set(proto, "Cube");
    TDF_Label assembly = shapes->NewShape();
    TDataStd_Name::Set(assembly, "Ensemble");
    gp_Trsf shift;
    shift.SetTranslation(gp_Vec(20.0, 0.0, 0.0));
    TDF_Label a = shapes->AddComponent(assembly, proto, TopLoc_Location());
    TDF_Label b = shapes->AddComponent(assembly, proto, TopLoc_Location(shift));
    TDataStd_Name::Set(a, "Cube gauche");
    TDataStd_Name::Set(b, "Cube droit");
    colors->SetColor(b, Quantity_Color(1.0, 0.0, 0.0, Quantity_TOC_RGB), XCAFDoc_ColorSurf);
    shapes->UpdateAssemblies();
    STEPCAFControl_Writer writer;
    writer.SetColorMode(true);
    writer.SetNameMode(true);
    const std::string file(path.data(), path.size());
    return writer.Transfer(doc, STEPControl_AsIs) && writer.Write(file.c_str()) == IFSelect_RetDone;
  } catch (...) {
    return false;
  }
}

} // namespace obv
