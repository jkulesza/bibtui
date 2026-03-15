use indexmap::IndexMap;

// ── Full-name → ISO 4 abbreviation table ─────────────────────────────────────
//
// Case-insensitive match on brace-stripped input.
// Entries are sorted roughly by discipline for readability.
static JOURNAL_ABBREVS: &[(&str, &str)] = &[
    // ── General science ───────────────────────────────────────────────────────
    ("Nature", "Nature"),
    ("Science", "Science"),
    ("Proceedings of the National Academy of Sciences", "Proc. Natl. Acad. Sci. USA"),
    ("Proceedings of the National Academy of Sciences of the United States of America", "Proc. Natl. Acad. Sci. USA"),
    ("Cell", "Cell"),
    ("The Lancet", "Lancet"),
    ("Lancet", "Lancet"),
    ("The New England Journal of Medicine", "N. Engl. J. Med."),
    ("New England Journal of Medicine", "N. Engl. J. Med."),
    ("PLOS ONE", "PLoS ONE"),
    ("PLoS ONE", "PLoS ONE"),
    ("Scientific Reports", "Sci. Rep."),
    ("Nature Communications", "Nat. Commun."),
    ("eLife", "eLife"),
    ("PLOS Biology", "PLoS Biol."),
    ("Nature Methods", "Nat. Methods"),
    ("Nature Biotechnology", "Nat. Biotechnol."),
    ("Nature Chemistry", "Nat. Chem."),
    ("Nature Materials", "Nat. Mater."),
    ("Nature Physics", "Nat. Phys."),
    ("Nature Photonics", "Nat. Photonics"),
    ("Nature Nanotechnology", "Nat. Nanotechnol."),
    ("Nature Genetics", "Nat. Genet."),
    ("Nature Medicine", "Nat. Med."),
    ("Nature Reviews Physics", "Nat. Rev. Phys."),
    ("Nature Reviews Materials", "Nat. Rev. Mater."),

    // ── Physics ───────────────────────────────────────────────────────────────
    ("Physical Review Letters", "Phys. Rev. Lett."),
    ("Physical Review A", "Phys. Rev. A"),
    ("Physical Review B", "Phys. Rev. B"),
    ("Physical Review C", "Phys. Rev. C"),
    ("Physical Review D", "Phys. Rev. D"),
    ("Physical Review E", "Phys. Rev. E"),
    ("Physical Review X", "Phys. Rev. X"),
    ("Physical Review Applied", "Phys. Rev. Appl."),
    ("Physical Review Accelerators and Beams", "Phys. Rev. Accel. Beams"),
    ("Physical Review Fluids", "Phys. Rev. Fluids"),
    ("Physical Review Materials", "Phys. Rev. Mater."),
    ("Physical Review Research", "Phys. Rev. Res."),
    ("Reviews of Modern Physics", "Rev. Mod. Phys."),
    ("Applied Physics Letters", "Appl. Phys. Lett."),
    ("Journal of Applied Physics", "J. Appl. Phys."),
    ("Annals of Physics", "Ann. Phys."),
    ("Journal of Physics A: Mathematical and Theoretical", "J. Phys. A: Math. Theor."),
    ("Journal of Physics B: Atomic, Molecular and Optical Physics", "J. Phys. B: At. Mol. Opt. Phys."),
    ("Journal of Physics: Condensed Matter", "J. Phys.: Condens. Matter"),
    ("Journal of Physics G: Nuclear and Particle Physics", "J. Phys. G: Nucl. Part. Phys."),
    ("European Physical Journal A", "Eur. Phys. J. A"),
    ("European Physical Journal B", "Eur. Phys. J. B"),
    ("European Physical Journal C", "Eur. Phys. J. C"),
    ("European Physical Journal D", "Eur. Phys. J. D"),
    ("European Physical Journal E", "Eur. Phys. J. E"),
    ("European Physical Journal Plus", "Eur. Phys. J. Plus"),
    ("European Physical Journal Special Topics", "Eur. Phys. J. Spec. Top."),
    ("New Journal of Physics", "New J. Phys."),
    ("Journal of High Energy Physics", "J. High Energy Phys."),
    ("Nuclear Physics A", "Nucl. Phys. A"),
    ("Nuclear Physics B", "Nucl. Phys. B"),
    ("Physics of Plasmas", "Phys. Plasmas"),
    ("Physics Letters A", "Phys. Lett. A"),
    ("Physics Letters B", "Phys. Lett. B"),
    ("Physics Reports", "Phys. Rep."),
    ("Progress in Particle and Nuclear Physics", "Prog. Part. Nucl. Phys."),
    ("Reports on Progress in Physics", "Rep. Prog. Phys."),
    ("Physica A: Statistical Mechanics and its Applications", "Physica A"),
    ("Physica B: Condensed Matter", "Physica B"),
    ("Physica C: Superconductivity and its Applications", "Physica C"),
    ("Physica D: Nonlinear Phenomena", "Physica D"),
    ("Physica E: Low-dimensional Systems and Nanostructures", "Physica E"),
    ("Solid State Communications", "Solid State Commun."),
    ("Solid State Physics", "Solid State Phys."),
    ("Surface Science", "Surf. Sci."),
    ("Thin Solid Films", "Thin Solid Films"),
    ("Superconductor Science and Technology", "Supercond. Sci. Technol."),
    ("Semiconductor Science and Technology", "Semicond. Sci. Technol."),
    ("Optics Letters", "Opt. Lett."),
    ("Optics Express", "Opt. Express"),
    ("Optics Communications", "Opt. Commun."),
    ("Journal of the Optical Society of America A", "J. Opt. Soc. Am. A"),
    ("Journal of the Optical Society of America B", "J. Opt. Soc. Am. B"),
    ("Laser and Particle Beams", "Laser Part. Beams"),
    ("Plasma Physics and Controlled Fusion", "Plasma Phys. Controlled Fusion"),
    ("Fusion Engineering and Design", "Fusion Eng. Des."),
    ("Nuclear Fusion", "Nucl. Fusion"),

    // ── Nuclear / radiation ───────────────────────────────────────────────────
    ("Nuclear Science and Engineering", "Nucl. Sci. Eng."),
    ("Nuclear Technology", "Nucl. Technol."),
    ("Annals of Nuclear Energy", "Ann. Nucl. Energy"),
    ("Nuclear Engineering and Design", "Nucl. Eng. Des."),
    ("Nuclear Instruments and Methods in Physics Research A", "Nucl. Instrum. Methods Phys. Res. A"),
    ("Nuclear Instruments and Methods in Physics Research B", "Nucl. Instrum. Methods Phys. Res. B"),
    ("Nuclear Instruments and Methods in Physics Research Section A", "Nucl. Instrum. Methods Phys. Res. A"),
    ("Nuclear Instruments and Methods in Physics Research Section B", "Nucl. Instrum. Methods Phys. Res. B"),
    ("Nuclear Instruments and Methods", "Nucl. Instrum. Methods"),
    ("Radiation Physics and Chemistry", "Radiat. Phys. Chem."),
    ("Radiation Measurements", "Radiat. Meas."),
    ("Radiation Protection Dosimetry", "Radiat. Prot. Dosimetry"),
    ("Journal of Radioanalytical and Nuclear Chemistry", "J. Radioanal. Nucl. Chem."),
    ("Progress in Nuclear Energy", "Prog. Nucl. Energy"),
    ("Journal of Nuclear Materials", "J. Nucl. Mater."),
    ("Journal of Nuclear Science and Technology", "J. Nucl. Sci. Technol."),
    ("Health Physics", "Health Phys."),
    ("Radiation Research", "Radiat. Res."),
    ("Applied Radiation and Isotopes", "Appl. Radiat. Isot."),
    ("IEEE Transactions on Nuclear Science", "IEEE Trans. Nucl. Sci."),
    ("Nuclear Data Sheets", "Nucl. Data Sheets"),
    ("Atomic Data and Nuclear Data Tables", "At. Data Nucl. Data Tables"),
    ("Energy Conversion and Management", "Energy Convers. Manage."),
    ("Progress in Nuclear Science and Technology", "Prog. Nucl. Sci. Technol."),
    ("EPJ Nuclear Sciences and Technologies", "EPJ Nucl. Sci. Technol."),

    // ── Chemistry ─────────────────────────────────────────────────────────────
    ("Journal of Chemical Physics", "J. Chem. Phys."),
    ("Journal of the American Chemical Society", "J. Am. Chem. Soc."),
    ("Journal of Physical Chemistry A", "J. Phys. Chem. A"),
    ("Journal of Physical Chemistry B", "J. Phys. Chem. B"),
    ("Journal of Physical Chemistry C", "J. Phys. Chem. C"),
    ("Journal of Physical Chemistry Letters", "J. Phys. Chem. Lett."),
    ("Chemical Physics Letters", "Chem. Phys. Lett."),
    ("Chemical Physics", "Chem. Phys."),
    ("Physical Chemistry Chemical Physics", "Phys. Chem. Chem. Phys."),
    ("Chemical Reviews", "Chem. Rev."),
    ("Chemical Society Reviews", "Chem. Soc. Rev."),
    ("Angewandte Chemie International Edition", "Angew. Chem. Int. Ed."),
    ("Journal of Chemical Theory and Computation", "J. Chem. Theory Comput."),
    ("Journal of Computational Chemistry", "J. Comput. Chem."),
    ("International Journal of Quantum Chemistry", "Int. J. Quantum Chem."),
    ("Molecular Physics", "Mol. Phys."),
    ("Journal of Molecular Spectroscopy", "J. Mol. Spectrosc."),
    ("Spectrochimica Acta Part A", "Spectrochim. Acta A"),
    ("Spectrochim. Acta Part A: Molecular and Biomolecular Spectroscopy", "Spectrochim. Acta A"),
    ("Inorganic Chemistry", "Inorg. Chem."),
    ("Dalton Transactions", "Dalton Trans."),
    ("Journal of the Chemical Society", "J. Chem. Soc."),
    ("Electrochimica Acta", "Electrochim. Acta"),
    ("Journal of Electroanalytical Chemistry", "J. Electroanal. Chem."),
    ("Talanta", "Talanta"),
    ("Analytical Chemistry", "Anal. Chem."),
    ("Analytica Chimica Acta", "Anal. Chim. Acta"),

    // ── Mathematics / CS ─────────────────────────────────────────────────────
    ("Mathematics of Computation", "Math. Comput."),
    ("Journal of Computational Physics", "J. Comput. Phys."),
    ("Communications of the ACM", "Commun. ACM"),
    ("SIAM Journal on Numerical Analysis", "SIAM J. Numer. Anal."),
    ("SIAM Journal on Scientific Computing", "SIAM J. Sci. Comput."),
    ("SIAM Journal on Applied Mathematics", "SIAM J. Appl. Math."),
    ("SIAM Review", "SIAM Rev."),
    ("Numerische Mathematik", "Numer. Math."),
    ("Numerical Methods for Partial Differential Equations", "Numer. Methods Partial Differ. Equ."),
    ("Journal of Scientific Computing", "J. Sci. Comput."),
    ("Applied Mathematics and Computation", "Appl. Math. Comput."),
    ("Applied Numerical Mathematics", "Appl. Numer. Math."),
    ("Computer Methods in Applied Mechanics and Engineering", "Comput. Methods Appl. Mech. Eng."),
    ("Journal of Computational and Applied Mathematics", "J. Comput. Appl. Math."),
    ("Journal of the ACM", "J. ACM"),
    ("ACM Transactions on Mathematical Software", "ACM Trans. Math. Softw."),
    ("IEEE Transactions on Computers", "IEEE Trans. Comput."),
    ("IEEE Transactions on Information Theory", "IEEE Trans. Inf. Theory"),
    ("Neural Networks", "Neural Netw."),
    ("Artificial Intelligence", "Artif. Intell."),
    ("Machine Learning", "Mach. Learn."),
    ("Journal of Machine Learning Research", "J. Mach. Learn. Res."),

    // ── Engineering ──────────────────────────────────────────────────────────
    ("International Journal of Heat and Mass Transfer", "Int. J. Heat Mass Transfer"),
    ("Proceedings of the IEEE", "Proc. IEEE"),
    ("IEEE Transactions on Signal Processing", "IEEE Trans. Signal Process."),
    ("IEEE Transactions on Automatic Control", "IEEE Trans. Autom. Control"),
    ("IEEE Transactions on Magnetics", "IEEE Trans. Magn."),
    ("IEEE Transactions on Power Systems", "IEEE Trans. Power Syst."),
    ("International Journal of Heat and Fluid Flow", "Int. J. Heat Fluid Flow"),
    ("International Journal of Multiphase Flow", "Int. J. Multiphase Flow"),
    ("International Journal of Thermal Sciences", "Int. J. Therm. Sci."),
    ("Journal of Fluid Mechanics", "J. Fluid Mech."),
    ("Physics of Fluids", "Phys. Fluids"),
    ("Flow, Turbulence and Combustion", "Flow Turbul. Combust."),
    ("Combustion and Flame", "Combust. Flame"),
    ("Experimental Thermal and Fluid Science", "Exp. Therm. Fluid Sci."),
    ("Journal of Heat Transfer", "J. Heat Transfer"),
    ("Journal of Turbomachinery", "J. Turbomach."),
    ("Acta Materialia", "Acta Mater."),
    ("Acta Metallurgica", "Acta Metall."),
    ("Journal of Materials Science", "J. Mater. Sci."),
    ("Materials Science and Engineering A", "Mater. Sci. Eng. A"),
    ("Materials Letters", "Mater. Lett."),
    ("Corrosion Science", "Corros. Sci."),
    ("Journal of Alloys and Compounds", "J. Alloys Compd."),
    ("Journal of the Mechanics and Physics of Solids", "J. Mech. Phys. Solids"),
    ("International Journal of Solids and Structures", "Int. J. Solids Struct."),
    ("Composites Science and Technology", "Compos. Sci. Technol."),
    ("Progress in Aerospace Sciences", "Prog. Aerosp. Sci."),
    ("Aerospace Science and Technology", "Aerosp. Sci. Technol."),
    ("AIAA Journal", "AIAA J."),

    // ── Energy / environment ──────────────────────────────────────────────────
    ("Energy", "Energy"),
    ("Energy and Environmental Science", "Energy Environ. Sci."),
    ("Applied Energy", "Appl. Energy"),
    ("Renewable Energy", "Renewable Energy"),
    ("Solar Energy", "Sol. Energy"),
    ("Solar Energy Materials and Solar Cells", "Sol. Energy Mater. Sol. Cells"),
    ("Journal of Power Sources", "J. Power Sources"),
    ("Environmental Science and Technology", "Environ. Sci. Technol."),
    ("Water Research", "Water Res."),
    ("Atmospheric Environment", "Atmos. Environ."),
    ("Atmospheric Chemistry and Physics", "Atmos. Chem. Phys."),

    // ── Biology / medicine ────────────────────────────────────────────────────
    ("Nucleic Acids Research", "Nucleic Acids Res."),
    ("Journal of Biological Chemistry", "J. Biol. Chem."),
    ("Biochemistry", "Biochemistry"),
    ("Biophysical Journal", "Biophys. J."),
    ("Bioinformatics", "Bioinformatics"),
    ("PLOS Genetics", "PLoS Genet."),
    ("PLOS Computational Biology", "PLoS Comput. Biol."),
    ("Genome Research", "Genome Res."),
    ("Molecular Cell", "Mol. Cell"),
    ("Journal of Cell Biology", "J. Cell Biol."),

    // ── Geophysics / astronomy ────────────────────────────────────────────────
    ("The Astrophysical Journal", "Astrophys. J."),
    ("Astrophysical Journal", "Astrophys. J."),
    ("Astrophysical Journal Letters", "Astrophys. J. Lett."),
    ("Astrophysical Journal Supplement Series", "Astrophys. J. Suppl. Ser."),
    ("Monthly Notices of the Royal Astronomical Society", "Mon. Not. R. Astron. Soc."),
    ("Astronomy and Astrophysics", "Astron. Astrophys."),
    ("The Astronomical Journal", "Astron. J."),
    ("Astronomical Journal", "Astron. J."),
    ("Geophysical Research Letters", "Geophys. Res. Lett."),
    ("Journal of Geophysical Research", "J. Geophys. Res."),
];

// ── LTWA word-level abbreviation table ───────────────────────────────────────
//
// `None` means the word is a stop word and should be dropped.
// `Some(abbr)` means replace with the given abbreviation (including trailing dot if needed).
static LTWA_WORDS: &[(&str, Option<&str>)] = &[
    // Stop words (dropped)
    ("a", None),
    ("an", None),
    ("and", None),
    ("at", None),
    ("by", None),
    ("for", None),
    ("in", None),
    ("nor", None),
    ("of", None),
    ("on", None),
    ("or", None),
    ("the", None),
    ("to", None),
    ("with", None),
    ("its", None),
    ("their", None),
    ("this", None),
    ("that", None),
    ("from", None),

    // Common journal words — abbreviated
    ("abstract", Some("Abstr.")),
    ("abstracts", Some("Abstr.")),
    ("account", Some("Acc.")),
    ("accounts", Some("Acc.")),
    ("acta", Some("Acta")),
    ("actuators", Some("Actuators")),
    ("advances", Some("Adv.")),
    ("advanced", Some("Adv.")),
    ("aeronautical", Some("Aeronaut.")),
    ("aeronautics", Some("Aeronaut.")),
    ("aerospace", Some("Aerosp.")),
    ("analysis", Some("Anal.")),
    ("analytical", Some("Anal.")),
    ("annals", Some("Ann.")),
    ("applied", Some("Appl.")),
    ("applications", Some("Appl.")),
    ("archives", Some("Arch.")),
    ("aspects", Some("Aspects")),
    ("association", Some("Assoc.")),
    ("astrophysical", Some("Astrophys.")),
    ("astrophysics", Some("Astrophys.")),
    ("atmospheric", Some("Atmos.")),
    ("atomic", Some("At.")),
    ("automation", Some("Autom.")),
    ("automatic", Some("Autom.")),
    ("behavior", Some("Behav.")),
    ("behaviour", Some("Behav.")),
    ("biochemical", Some("Biochem.")),
    ("biochemistry", Some("Biochem.")),
    ("biological", Some("Biol.")),
    ("biology", Some("Biol.")),
    ("biomedical", Some("Biomed.")),
    ("biophysical", Some("Biophys.")),
    ("biophysics", Some("Biophys.")),
    ("biotechnology", Some("Biotechnol.")),
    ("bulletin", Some("Bull.")),
    ("catalysis", Some("Catal.")),
    ("catalysts", Some("Catal.")),
    ("ceramic", Some("Ceram.")),
    ("ceramics", Some("Ceram.")),
    ("characterization", Some("Charact.")),
    ("chemical", Some("Chem.")),
    ("chemistry", Some("Chem.")),
    ("chinese", Some("Chin.")),
    ("circulation", Some("Circ.")),
    ("clinical", Some("Clin.")),
    ("colloid", Some("Colloid")),
    ("communications", Some("Commun.")),
    ("computational", Some("Comput.")),
    ("computation", Some("Comput.")),
    ("computer", Some("Comput.")),
    ("computers", Some("Comput.")),
    ("computing", Some("Comput.")),
    ("condensed", Some("Condens.")),
    ("control", Some("Control")),
    ("current", Some("Curr.")),
    ("data", Some("Data")),
    ("design", Some("Des.")),
    ("dynamics", Some("Dyn.")),
    ("electroanalytical", Some("Electroanal.")),
    ("electrochemical", Some("Electrochem.")),
    ("electrochemistry", Some("Electrochem.")),
    ("electronic", Some("Electron.")),
    ("electronics", Some("Electron.")),
    ("energy", Some("Energy")),
    ("engineering", Some("Eng.")),
    ("environmental", Some("Environ.")),
    ("environment", Some("Environ.")),
    ("european", Some("Eur.")),
    ("experimental", Some("Exp.")),
    ("experiments", Some("Exp.")),
    ("fluids", Some("Fluids")),
    ("fluid", Some("Fluid")),
    ("fundamentals", Some("Fundam.")),
    ("general", Some("Gen.")),
    ("geophysical", Some("Geophys.")),
    ("geophysics", Some("Geophys.")),
    ("hazardous", Some("Hazard.")),
    ("health", Some("Health")),
    ("high", Some("High")),
    ("hydrology", Some("Hydrol.")),
    ("hydrological", Some("Hydrol.")),
    ("ieee", Some("IEEE")),
    ("imaging", Some("Imaging")),
    ("inorganic", Some("Inorg.")),
    ("instrumentation", Some("Instrum.")),
    ("instruments", Some("Instrum.")),
    ("international", Some("Int.")),
    ("investigation", Some("Invest.")),
    ("investigations", Some("Invest.")),
    ("journal", Some("J.")),
    ("laser", Some("Laser")),
    ("lasers", Some("Lasers")),
    ("letters", Some("Lett.")),
    ("low", Some("Low")),
    ("management", Some("Manage.")),
    ("materials", Some("Mater.")),
    ("material", Some("Mater.")),
    ("mathematical", Some("Math.")),
    ("mathematics", Some("Math.")),
    ("mechanical", Some("Mech.")),
    ("mechanics", Some("Mech.")),
    ("medical", Some("Med.")),
    ("medicine", Some("Med.")),
    ("mesoscale", Some("Mesoscale")),
    ("methods", Some("Methods")),
    ("modelling", Some("Model.")),
    ("modeling", Some("Model.")),
    ("molecular", Some("Mol.")),
    ("nanoscale", Some("Nanoscale")),
    ("nanotechnology", Some("Nanotechnol.")),
    ("national", Some("Natl.")),
    ("new", Some("New")),
    ("nonlinear", Some("Nonlinear")),
    ("nuclear", Some("Nucl.")),
    ("numerical", Some("Numer.")),
    ("optics", Some("Opt.")),
    ("optical", Some("Opt.")),
    ("organic", Some("Org.")),
    ("particle", Some("Part.")),
    ("particles", Some("Part.")),
    ("photochemistry", Some("Photochem.")),
    ("photochemical", Some("Photochem.")),
    ("photonics", Some("Photonics")),
    ("physical", Some("Phys.")),
    ("physics", Some("Phys.")),
    ("plasma", Some("Plasma")),
    ("plasmas", Some("Plasma")),
    ("pollution", Some("Pollut.")),
    ("polymer", Some("Polym.")),
    ("polymers", Some("Polym.")),
    ("power", Some("Power")),
    ("proceedings", Some("Proc.")),
    ("process", Some("Process.")),
    ("processing", Some("Process.")),
    ("progress", Some("Prog.")),
    ("protection", Some("Prot.")),
    ("quantum", Some("Quantum")),
    ("radiation", Some("Radiat.")),
    ("radiative", Some("Radiat.")),
    ("radioanalytical", Some("Radioanal.")),
    ("renewable", Some("Renewable")),
    ("reports", Some("Rep.")),
    ("research", Some("Res.")),
    ("review", Some("Rev.")),
    ("reviews", Some("Rev.")),
    ("royal", Some("R.")),
    ("science", Some("Sci.")),
    ("sciences", Some("Sci.")),
    ("scientific", Some("Sci.")),
    ("semiconductor", Some("Semicond.")),
    ("semiconductors", Some("Semicond.")),
    ("simulation", Some("Simul.")),
    ("simulations", Some("Simul.")),
    ("society", Some("Soc.")),
    ("solar", Some("Sol.")),
    ("solid", Some("Solid")),
    ("spectroscopy", Some("Spectrosc.")),
    ("spectroscopic", Some("Spectrosc.")),
    ("surface", Some("Surf.")),
    ("surfaces", Some("Surf.")),
    ("systems", Some("Syst.")),
    ("system", Some("Syst.")),
    ("technology", Some("Technol.")),
    ("technologies", Some("Technol.")),
    ("theoretical", Some("Theor.")),
    ("theory", Some("Theory")),
    ("thermal", Some("Therm.")),
    ("thermodynamics", Some("Thermodyn.")),
    ("transactions", Some("Trans.")),
    ("transport", Some("Transp.")),
    ("turbulence", Some("Turbul.")),
    ("turbulent", Some("Turbul.")),
];

// ── Public API ────────────────────────────────────────────────────────────────

/// Strip BibTeX case-protecting braces for lookup purposes.
/// Removes all `{` and `}` characters.
fn strip_braces(s: &str) -> String {
    s.chars().filter(|&c| c != '{' && c != '}').collect()
}

/// Look up ISO 4 abbreviation for a journal name.
///
/// Priority:
/// 1. User-supplied `overrides` (case-insensitive, checked first)
/// 2. Built-in `JOURNAL_ABBREVS` table (case-insensitive, brace-stripped)
/// 3. LTWA word-level fallback
///
/// Returns an empty string for empty input.
pub fn abbreviate_journal(name: &str, overrides: &IndexMap<String, String>) -> String {
    if name.is_empty() {
        return String::new();
    }

    let stripped = strip_braces(name);
    let lower = stripped.to_lowercase();

    // 1. User overrides (case-insensitive)
    for (k, v) in overrides {
        if k.to_lowercase() == lower {
            return v.clone();
        }
    }

    // 2. Built-in table (case-insensitive)
    for &(full, abbrev) in JOURNAL_ABBREVS {
        if full.to_lowercase() == lower {
            return abbrev.to_string();
        }
    }

    // 3. LTWA word-level fallback
    ltwa_abbreviate(&stripped)
}

/// Apply LTWA word-by-word abbreviation to an unrecognized journal name.
fn ltwa_abbreviate(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.is_empty() {
        return name.to_string();
    }

    let mut parts: Vec<&str> = Vec::new();
    for word in &words {
        let lower = word.to_lowercase();
        if let Some(&(_k, abbr_opt)) = LTWA_WORDS.iter().find(|&&(k, _)| k == lower.as_str()) {
            if let Some(abbr) = abbr_opt {
                parts.push(abbr);
            }
            // None = stop word, dropped
        } else {
            // Unknown word: keep as-is
            parts.push(word);
        }
    }

    if parts.is_empty() {
        // All words were stop words — return original stripped name
        name.to_string()
    } else {
        parts.join(" ")
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn no_overrides() -> IndexMap<String, String> {
        IndexMap::new()
    }

    #[test]
    fn known_full_name() {
        let result = abbreviate_journal("Physical Review Letters", &no_overrides());
        assert_eq!(result, "Phys. Rev. Lett.");
    }

    #[test]
    fn case_insensitive_match() {
        let result = abbreviate_journal("PHYSICAL REVIEW LETTERS", &no_overrides());
        assert_eq!(result, "Phys. Rev. Lett.");
    }

    #[test]
    fn brace_stripped_input() {
        let result = abbreviate_journal("{Physical Review Letters}", &no_overrides());
        assert_eq!(result, "Phys. Rev. Lett.");
    }

    #[test]
    fn nuclear_science_engineering() {
        let result = abbreviate_journal("Nuclear Science and Engineering", &no_overrides());
        assert_eq!(result, "Nucl. Sci. Eng.");
    }

    #[test]
    fn unknown_journal_ltwa_fallback() {
        // "Exotic Journal of Widgets" → LTWA fallback
        let result = abbreviate_journal("Journal of Applied Widgets", &no_overrides());
        // "Journal" → "J.", "of" → dropped, "Applied" → "Appl.", "Widgets" → "Widgets"
        assert_eq!(result, "J. Appl. Widgets");
    }

    #[test]
    fn ltwa_stop_word_only_returns_original() {
        // Pure stop words → original name returned
        let result = abbreviate_journal("In and of the", &no_overrides());
        assert_eq!(result, "In and of the");
    }

    #[test]
    fn single_word_kept_as_is() {
        // "Nature" is in the table
        let result = abbreviate_journal("Nature", &no_overrides());
        assert_eq!(result, "Nature");
    }

    #[test]
    fn user_override_takes_precedence() {
        let mut overrides = IndexMap::new();
        overrides.insert("Nature".to_string(), "Nat.".to_string());
        let result = abbreviate_journal("Nature", &overrides);
        assert_eq!(result, "Nat.");
    }

    #[test]
    fn user_override_case_insensitive() {
        let mut overrides = IndexMap::new();
        overrides.insert("nuclear science and engineering".to_string(), "NSE".to_string());
        let result = abbreviate_journal("Nuclear Science and Engineering", &overrides);
        assert_eq!(result, "NSE");
    }

    #[test]
    fn empty_input_returns_empty() {
        let result = abbreviate_journal("", &no_overrides());
        assert_eq!(result, "");
    }

    #[test]
    fn annals_of_nuclear_energy() {
        let result = abbreviate_journal("Annals of Nuclear Energy", &no_overrides());
        assert_eq!(result, "Ann. Nucl. Energy");
    }

    #[test]
    fn journal_computational_physics() {
        let result = abbreviate_journal("Journal of Computational Physics", &no_overrides());
        assert_eq!(result, "J. Comput. Phys.");
    }

    #[test]
    fn whitespace_only_returns_unchanged() {
        // Non-empty but whitespace-only: passes the early-return guard, reaches
        // ltwa_abbreviate, split_whitespace yields nothing → original returned.
        let result = abbreviate_journal("   ", &no_overrides());
        assert_eq!(result, "   ");
    }

    #[test]
    fn internal_braces_stripped_for_lookup() {
        // Braces anywhere in the string (not just wrapping the whole value) are
        // stripped before lookup so the match still works.
        let result = abbreviate_journal("{Physical Review} Letters", &no_overrides());
        // After brace-stripping: "Physical Review Letters" → table match
        assert_eq!(result, "Phys. Rev. Lett.");
    }

    #[test]
    fn ltwa_known_stop_word_dropped_with_other_words() {
        // "the" is a stop word (None) — it must be dropped, not kept.
        let result = abbreviate_journal("Annals of the Nuclear Energy Society", &no_overrides());
        // "Annals"→"Ann.", "of"→drop, "the"→drop, "Nuclear"→"Nucl.", "Energy"→"Energy", "Society"→"Soc."
        assert_eq!(result, "Ann. Nucl. Energy Soc.");
    }

    #[test]
    fn ltwa_all_known_abbreviations() {
        // Exercise a selection of LTWA_WORDS entries not covered by other tests.
        let result = abbreviate_journal("International Journal of Engineering Research", &no_overrides());
        assert_eq!(result, "Int. J. Eng. Res.");
    }
}
