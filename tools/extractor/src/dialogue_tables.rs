//! Per-language text and font tables, transcribed from
//! `tables/text_compression.py` (`kTextAlphabet_*`, `kTextDictionary_*`,
//! `kText_Command*`, the `Lang*` classes and `kLanguages`) and
//! `tables/sprite_sheets.py:148-160` (`kFontTypes`).
//!
//! These were generated mechanically from the Python source rather than typed
//! out, because the *order* of `dictionary` is load-bearing: the greedy
//! encoder takes the first entry that is a prefix, not the longest, so a
//! reordering silently changes the compressed bytes.

/// One entry of `text_compression.kLanguages`.
pub struct Lang {
    pub code: &'static str,
    pub alphabet: &'static [&'static str],
    pub dictionary: &'static [&'static str],
    pub command_lengths: &'static [u8],
    pub command_names: &'static [&'static str],
    pub rom_addrs: [u32; 2],
    pub command_start: u8,
    pub switch_bank: u8,
    pub finish: u8,
    pub dict_base_enc: u8,
    pub dict_base_dec: u8,
    pub escape: Option<u8>,
    /// `Lang.encoder == 'new'`; also what `uses_new_format()` reports.
    pub new_encoder: bool,
}

/// One entry of `sprite_sheets.kFontTypes`. The PNG filename is dropped: the
/// round trip through it is the identity (asserted by `decode_font:199`), so
/// the two ROM reads below are the whole of it.
pub struct Font {
    pub code: &'static str,
    pub gfx: u32,
    pub tiles: usize,
    pub widths_addr: u32,
    pub chars: usize,
}

pub const ALPHABET_US: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "[...]", ">", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[Up]", "[Down]", "[Left]", "[Right]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "<", "[A]", "[B]", "[X]", "[Y]"];
pub const DICT_US: &[&str] = &["    ", "   ", "  ", "'s ", "and ", "are ", "all ", "ain", "and", "at ", "ast", "an", "at", "ble", "ba", "be", "bo", "can ", "che", "com", "ck", "des", "di", "do", "en ", "er ", "ear", "ent", "ed ", "en", "er", "ev", "for", "fro", "give ", "get", "go", "have", "has", "her", "hi", "ha", "ight ", "ing ", "in", "is", "it", "just", "know", "ly ", "la", "lo", "man", "ma", "me", "mu", "n't ", "non", "not", "open", "ound", "out ", "of", "on", "or", "per", "ple", "pow", "pro", "re ", "re", "some", "se", "sh", "so", "st", "ter ", "thin", "ter", "tha", "the", "thi", "to", "tr", "up", "ver", "with", "wa", "we", "wh", "wi", "you", "Her", "Tha", "The", "Thi", "You"];
pub const ALPHABET_DE: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "[...]", ">", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[UpL]", "[UpR]", "[LeftL]", "[LeftR]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "ö", "[A]", "[B]", "[X]", "[Y]", "ü", "ß", ":", "[DownL]", "[DownR]", "[RightL]", "[RightR]", "è", "é", "ê", "à", "ù", "ç", "Ä", "Ö", "Ü", "ä"];
pub const DICT_DE: &[&str] = &["    ", "   ", "                                          ", "-Knopf", " ich ", " Sch", " Ver", " zu ", " es ", "aber", "alle", "auch", "ang", "aus", "auf", "an", "bist", "bin", "bei", "der ", "die ", "das ", "den ", "dem ", "daß", "der", "die", "das", "den", "da", "etwas", "ein ", "ein", "en ", "er ", "es ", "en", "er", "es", "ei", "für", "fe", "habe", "hier", "hast", "her", "ich ", "icht", "ich", "ist", "ie ", "im", "ie", "kannst ", "kannst", "kommen", "kann ", "ll", "mich", "mein", "mit", "mal", "mir", "nicht ", "nicht", "nen", "nn", "och ", "och", "or", "schon", "sich", "sein", "sch", "sie", "st", "tte", "te ", "te", "und ", "und", "ung", "um", "von", "ver", "vor", "wird", "zu ", "Amulett", "Aber", "Deine", "Dich ", "Dir ", "Dir", "Der", "Die", "Das", "Du ", "Du", "Da", "Ein", "Hyrule", "Hier", "Ich ", "Master-Schwert", "Mach", "Rubine", "Sch", "Sie", "Ver", "Weisen", "Zelda"];
pub const ALPHABET_FR: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "[...]", ">", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[UpL]", "[UpR]", "[LeftL]", "[LeftR]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "ö", "[A]", "[B]", "[X]", "[Y]", "ü", "ô", ":", "[DownL]", "[DownR]", "[RightL]", "[RightR]", "è", "é", "ê", "à", "ù", "ç", "â", "û", "î", "ä"];
pub const DICT_FR: &[&str] = &["                                          ", " de ", " la ", " le ", " ! ", " d", " p", " t", " !", ", c'est moi, Sahasrahla", ", ", "ais ", "as ", "an", "ai", "a ", "che", "ce", "ch", "dans ", "des ", "de ", "de", "est ", "ent", "en ", "er ", "es ", "en", "es", "et", "eu", "e,", "e ", "ique", "ien", "is ", "ie", "in", "ir", "is", "i ", "les ", "la ", "le ", "le", "ll", "maintenant", "magique", "ment", "mon", "mai", "me", "ne ", "onne", "oir", "our", "ouv", "oi", "on", "ou", "or", "pouvoir", "pour", "peux", "pas", "que ", "qu", "rubis", "re ", "ra", "re", "r ", "sorcier", "s l", "s d", "se", "so", "s ", "tro", "te ", "tu ", "te", "t ", "un", "ur", "u ", "ver", "Ah ! Ah ! Ah !", "C'est", "Ganon", "Maintenant", "Merci", "Monde", "Perle de Lune", "Tu as trouvé ", "Ténèbres", "Tu peux", "Tu "];
pub const ALPHABET_FR_C: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "[...]", ">", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[UpL]", "[UpR]", "[LeftL]", "[LeftR]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "ö", "[A]", "[B]", "[X]", "[Y]", "ü", "ô", ":", "[DownL]", "[DownR]", "[RightL]", "[RightR]", "è", "é", "ê", "à", "ù", "ç", "â", "û", "î", "ä"];
pub const DICT_FR_C: &[&str] = &["                                          ", " de ", " la ", " le ", " ! ", " d", " p", " t", " !", ", c'est moi, Sahasrahla", ", ", "ais ", "as ", "an", "ai", "a ", "che", "ce", "ch", "dans ", "des ", "de ", "de", "est ", "ent", "en ", "er ", "es ", "en", "es", "et", "eu", "e,", "e ", "ique", "ien", "is ", "ie", "in", "ir", "is", "i ", "les ", "la ", "le ", "le", "ll", "maintenant", "magique", "ment", "mon", "mai", "me", "ne ", "onne", "oir", "our", "ouv", "oi", "on", "ou", "or", "pouvoir", "pour", "peux", "pas", "que ", "qu", "rubis", "re ", "ra", "re", "r ", "sorcier", "s l", "s d", "se", "so", "s ", "tro", "te ", "tu ", "te", "t ", "un", "ur", "u ", "ver", "Ah ! Ah ! Ah !", "C'est", "Ganon", "Maintenant", "Merci", "Monde", "Perle de Lune", "Tu as trouvé ", "Ténèbres", "Tu peux", "Tu "];
pub const ALPHABET_EN: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "[...]", ">", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[UpL]", "[UpR]", "[LeftL]", "[LeftR]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "ö", "[A]", "[B]", "[X]", "[Y]", "ü", "ß", ":", "[DownL]", "[DownR]", "[RightL]", "[RightR]", "è", "é", "ê", "à", "ù", "ç", "Ä", "Ö", "Ü", "ä"];
pub const DICT_EN: &[&str] = &["    ", "   ", "  ", "'s ", "and ", "are ", "all ", "ain", "and", "at ", "ast", "an", "at", "ble", "ba", "be", "bo", "can ", "che", "com", "ck", "des", "di", "do", "en ", "er ", "ear", "ent", "ed ", "en", "er", "ev", "for", "fro", "give ", "get", "go", "have", "has", "her", "hi", "ha", "ight ", "ing ", "in", "is", "it", "just", "know", "ly ", "la", "lo", "man", "ma", "me", "mu", "n't ", "non", "not", "open", "ound", "out ", "of", "on", "or", "per", "ple", "pow", "pro", "re ", "re", "some", "se", "sh", "so", "st", "ter ", "thin", "ter", "tha", "the", "thi", "to", "tr", "up", "ver", "with", "wa", "we", "wh", "wi", "you", "Her", "Tha", "The", "Thi", "You"];
pub const ALPHABET_ES: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "é", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "ó", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "[Waves]", ".", ",", "[...]", ">", "(", ")", "ñ", "ú", "á", "[LinkL]", "[LinkR]", "\"", "[Up]", "[Down]", "[Left]", "[Right]", "í", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[Ankh]", "[4HeartR]", " ", "[Snake]", "[A]", "[B]", "[X]", "[Y]", "[I]", "¡", "¿", "Ñ"];
pub const DICT_ES: &[&str] = &["    ", "   ", "  ", " en", " la ", " el ", " de ", "ien", "tra", " de", "te ", "ar", "a ", "ada", "es", "as", "o ", " con", "ero", "ado", "e ", "que", "en", "al", "os ", "ora", "nte", " al", "lo ", "or", "os", "er", "aci", "res", " que ", " es", "el", "los ", "tar", " se", ", ", "ro", " de l", " est", "re", "on", "an", "pued", " del", "ás ", "la", "ti", "la ", "Es", "to", "ta", "para", "uer", "ier", " un ", " por", "oder", "da", "in", "cu", " ha", "per", "ano", " ve", "cer", "lo", " no ", "ic", "ra", "ab", "ir", " una", "undo", "es ", "as ", "con", "a, ", "te", " m", "gu", " tu", "ando", " p", "de", "le", "ol", "o, ", "ten", "lle", " a ", "aba", "com"];
pub const ALPHABET_PL: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "ć", "[Right]", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[Up]", "[Down]", "ę", "ł", "ń", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "ą", "[4HeartR]", " ", "[Left]", "ó", "ś", "ż", "ź", "Ł", "Ś", "Ż", "Ź"];
pub const DICT_PL: &[&str] = &["Trój", "...", "ść", "Nie", " nie", " się", "może", " że", "and", "at ", " ty", "an", "at", "kus", "ba", "be", "bo", "chce", "che", "ki ", "za", "des", "di", "do", "en ", "er ", "sz ", "ent", "ed ", "en", "er", " w", "moc", "zię", "przez", "ale", "go", "dzie", "has", "rze", "hi", "ha", "który", "aby ", "in", "is", "it", "twoj", "Może", "łeś", "la", "lo", "czn", "ma", "me", "mu", "szcz", "ska", "śli", "przy", "znaj", "iecz", "of", "on", "or", "   ", "ple", "pow", "pro", "re ", "re", "mnie", "se", " z", "so", "st", "któr", " jak", "ksz", "sze", "coś", " je", "to", "tr", "up", "kie", "praw", "wa", "we", "mi", "wi", "szy", "chc", "pra", "cie", " i ", "esz"];
pub const ALPHABET_PT: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "[...]", ">", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[Up]", "[Down]", "[Left]", "[Right]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "<", "[A]", "[B]", "[X]", "[Y]", "[I]", "¡", "[!]", "Á", "À", "Â", "Ã", "É", "Ê", "Í", "Ó", "Ô", "Õ", "Ú", "á", "à", "â", "ã", "é", "ê", "í", "ó", "ô", "õ", "ú", "ç"];
pub const DICT_PT: &[&str] = &["     ", "    ", "   ", "                                          ", "o ", "a ", "e ", "..", "de", "ar", "s ", "ra", " d", "es", "ocê ", "do", " a", " p", "er", " e", "que", "r ", "os", "te", ", ", "as", "or", "m ", "en", " o", "nt", "re", " s", "co", "da", "se", "st", " c", " m", "em", "ma", "ta", " n", "ad", "on", "al", "ro", "an", "u ", "nd", " um", "pa", "ca", "el", " f", "to", "in", " t", "ou", "ei", "ss", "ir", "no", "ri", "tr", "me", "la", "ia", "le", "ve", "is", "sa", "eu", "pe", "a.", "na", "so", "mo", "ga", "o.", "á ", "lo", "ha", "pr", "ua", " l", "! ", "ui", "am", "ti", "io", "gu", "i ", "di", "nh", " i", "id"];
pub const ALPHABET_REDUX: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "[...]", ">", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[Up]", "[Down]", "[Left]", "[Right]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "<", "[A]", "[B]", "[X]", "[Y]"];
pub const DICT_REDUX: &[&str] = &["    ", "   ", "  ", "'s ", "and ", "are ", "all ", "ain", "and", "at ", "ast", "an", "at", "ble", "ba", "be", "bo", "can ", "che", "com", "ck", "des", "di", "do", "en ", "er ", "ear", "ent", "ed ", "en", "er", "ev", "for", "fro", "give ", "get", "go", "have", "has", "her", "hi", "ha", "ight ", "ing ", "in", "is", "it", "just", "know", "ly ", "la", "lo", "man", "ma", "me", "mu", "n't ", "non", "not", "open", "ound", "out ", "of", "on", "or", "per", "ple", "pow", "pro", "re ", "re", "some", "se", "sh", "so", "st", "ter ", "thin", "ter", "tha", "the", "thi", "to", "tr", "up", "ver", "with", "wa", "we", "wh", "wi", "you", "Her", "Tha", "The", "Thi", "You"];
pub const ALPHABET_NL: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "-", ".", ",", "[...]", ">", "(", ")", "[Ankh]", "[Waves]", "[Snake]", "[LinkL]", "[LinkR]", "\"", "[Up]", "[Down]", "[Left]", "[Right]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "<", "[A]", "[B]", "[X]", "[Y]"];
pub const DICT_NL: &[&str] = &["    ", "   ", "  ", "'s ", "and ", "are ", "all ", "ain", "and", "at ", "ast", "an", "at", "ble", "ba", "be", "bo", "can ", "che", "com", "ck", "des", "di", "do", "en ", "er ", "ear", "ent", "ed ", "en", "er", "ev", "for", "fro", "give ", "get", "go", "have", "has", "her", "hi", "ha", "ight ", "ing ", "in", "is", "it", "just", "know", "ly ", "la", "lo", "man", "ma", "me", "mu", "n't ", "non", "not", "open", "ound", "out ", "of", "on", "or", "per", "ple", "pow", "pro", "re ", "re", "some", "se", "sh", "so", "st", "ter ", "thin", "ter", "tha", "the", "thi", "to", "tr", "up", "ver", "with", "wa", "we", "wh", "wi", "you", "Her", "Tha", "The", "Thi", "You"];
pub const ALPHABET_SV: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "Ö", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "!", "?", "å", ".", ",", "ä", ">", "(", ")", "ö", "Å", "Ä", "[LinkL]", "[LinkR]", "\"", "[Up]", "[Down]", "[Left]", "[Right]", "'", "[1HeartL]", "[1HeartR]", "[2HeartL]", "[3HeartL]", "[3HeartR]", "[4HeartL]", "[4HeartR]", " ", "<", "[Ankh]", "[Waves]", "[Snake]", "-", "[I]", "[i]", "…", " "];
pub const DICT_SV: &[&str] = &["    ", "   ", "  ", "Du ", "till", "vill", "bara", "det", "den", "och", "en ", "r ", "n ", "ett", "en", " d", "a ", "Hjäl", "har", "ter", "t ", "var", " s", "de", "kan", "med", "som", "för", "att", "ar", " h", "er", "jag", "dig", "öppna", "mig", "är", "inte", "hit", "på ", "an", "e ", "rupie", "0kej", " m", "et", ", ", "gång", "måst", "ten", " f", "u ", "men", "te", "tt", "ka", "vara", "ken", "0m ", "från", "myck", "någo", "in", " k", " i", "vil", "bar", "ond", "För", "Jag", "ra", "tack", "ll", "g ", "ta", "om", "anna", "alla", "en,", "ber", "hem", "han", "st", "ig", " t", "tro", "kraf", "ör", " v", "ag", "… ", "får", "sin", "mme", "mma", "en ", "tat"];

/// `kLanguages`, in declaration order. That order is this port's canonical
/// language order, so the same set of ROMs always yields the same bytes.
pub const LANGS: &[Lang] = &[
    Lang { code: "us", alphabet: ALPHABET_US, dictionary: DICT_US,
        command_lengths: &[1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1],
        command_names: &["NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd", "Selchg", "Unused_Crash", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Unused_Mark", "Unused_Mark2", "Unused_Clear", "Waitkey"],
        rom_addrs: [0x9c8000, 0x8edf40], command_start: 0x67, switch_bank: 0x80,
        finish: 0xff, dict_base_enc: 0x88, dict_base_dec: 0x88,
        escape: None, new_encoder: false },
    Lang { code: "de", alphabet: ALPHABET_DE, dictionary: DICT_DE,
        command_lengths: &[1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
        command_names: &["Selchg", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Mark", "Mark2", "Clear", "Waitkey", "EndMessage", "NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd"],
        rom_addrs: [0x9c8000, 0x8ceb00], command_start: 0x70, switch_bank: 0x88,
        finish: 0x8f, dict_base_enc: 0x88, dict_base_dec: 0x90,
        escape: None, new_encoder: true },
    Lang { code: "fr", alphabet: ALPHABET_FR, dictionary: DICT_FR,
        command_lengths: &[1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
        command_names: &["Selchg", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Mark", "Mark2", "Clear", "Waitkey", "EndMessage", "NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd"],
        rom_addrs: [0x9c8000, 0x8ce800], command_start: 0x70, switch_bank: 0x88,
        finish: 0x8f, dict_base_enc: 0x88, dict_base_dec: 0x90,
        escape: None, new_encoder: true },
    Lang { code: "fr-c", alphabet: ALPHABET_FR_C, dictionary: DICT_FR_C,
        command_lengths: &[1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
        command_names: &["Selchg", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Mark", "Mark2", "Clear", "Waitkey", "EndMessage", "NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd"],
        rom_addrs: [0x9c8000, 0x8cf150], command_start: 0x70, switch_bank: 0x88,
        finish: 0x8f, dict_base_enc: 0x88, dict_base_dec: 0x90,
        escape: None, new_encoder: true },
    Lang { code: "en", alphabet: ALPHABET_EN, dictionary: DICT_EN,
        command_lengths: &[1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1],
        command_names: &["NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd", "Selchg", "Unused_Crash", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Unused_Mark", "Unused_Mark2", "Unused_Clear", "Waitkey"],
        rom_addrs: [0x9c8000, 0x8edf60], command_start: 0x67, switch_bank: 0x80,
        finish: 0xff, dict_base_enc: 0x88, dict_base_dec: 0x88,
        escape: None, new_encoder: false },
    Lang { code: "es", alphabet: ALPHABET_ES, dictionary: DICT_ES,
        command_lengths: &[1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1],
        command_names: &["NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd", "Selchg", "Unused_Crash", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Unused_Mark", "Unused_Mark2", "Unused_Clear", "Waitkey"],
        rom_addrs: [0x9c8000, 0x8edf40], command_start: 0x67, switch_bank: 0x80,
        finish: 0xff, dict_base_enc: 0x88, dict_base_dec: 0x88,
        escape: None, new_encoder: false },
    Lang { code: "pl", alphabet: ALPHABET_PL, dictionary: DICT_PL,
        command_lengths: &[1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1],
        command_names: &["NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd", "Selchg", "Unused_Crash", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Unused_Mark", "Unused_Mark2", "Unused_Clear", "Waitkey"],
        rom_addrs: [0x9c8000, 0x8edf40], command_start: 0x67, switch_bank: 0x80,
        finish: 0xff, dict_base_enc: 0x88, dict_base_dec: 0x88,
        escape: None, new_encoder: false },
    Lang { code: "pt", alphabet: ALPHABET_PT, dictionary: DICT_PT,
        command_lengths: &[1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1],
        command_names: &["NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd", "Selchg", "Unused_Crash", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Unused_Mark", "Unused_Mark2", "Unused_Clear", "Waitkey"],
        rom_addrs: [0x9c8000, 0x8edf40], command_start: 0x67, switch_bank: 0x80,
        finish: 0xff, dict_base_enc: 0x88, dict_base_dec: 0x88,
        escape: Some(0x62), new_encoder: true },
    Lang { code: "redux", alphabet: ALPHABET_REDUX, dictionary: DICT_REDUX,
        command_lengths: &[1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1],
        command_names: &["NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd", "Selchg", "Unused_Crash", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Unused_Mark", "Unused_Mark2", "Unused_Clear", "Waitkey"],
        rom_addrs: [0x9c8000, 0x8edf40], command_start: 0x67, switch_bank: 0x80,
        finish: 0xff, dict_base_enc: 0x88, dict_base_dec: 0x88,
        escape: None, new_encoder: false },
    Lang { code: "nl", alphabet: ALPHABET_NL, dictionary: DICT_NL,
        command_lengths: &[1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1],
        command_names: &["NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd", "Selchg", "Unused_Crash", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Unused_Mark", "Unused_Mark2", "Unused_Clear", "Waitkey"],
        rom_addrs: [0x9c8000, 0x8edf40], command_start: 0x67, switch_bank: 0x80,
        finish: 0xff, dict_base_enc: 0x88, dict_base_dec: 0x88,
        escape: None, new_encoder: false },
    Lang { code: "sv", alphabet: ALPHABET_SV, dictionary: DICT_SV,
        command_lengths: &[1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1, 1],
        command_names: &["NextPic", "Choose", "Item", "Name", "Window", "Number", "Position", "ScrollSpd", "Selchg", "Unused_Crash", "Choose3", "Choose2", "Scroll", "1", "2", "3", "Color", "Wait", "Sound", "Speed", "Unused_Mark", "Unused_Mark2", "Unused_Clear", "Waitkey"],
        rom_addrs: [0x9c8000, 0x8edf40], command_start: 0x67, switch_bank: 0x80,
        finish: 0xff, dict_base_enc: 0x88, dict_base_dec: 0x88,
        escape: None, new_encoder: false },
];

/// `sprite_sheets.kFontTypes`.
pub const FONTS: &[Font] = &[
    Font { code: "us", gfx: 0x8e8000, tiles: 256, widths_addr: 0x8ecadf, chars: 99 },
    Font { code: "de", gfx: 0xcc6e8, tiles: 256, widths_addr: 0x8cdecf, chars: 112 },
    Font { code: "fr", gfx: 0xcc6e8, tiles: 256, widths_addr: 0x8cdeaf, chars: 112 },
    Font { code: "fr-c", gfx: 0xcd078, tiles: 256, widths_addr: 0x8ce83f, chars: 112 },
    Font { code: "en", gfx: 0x8e8000, tiles: 256, widths_addr: 0x8ecaff, chars: 102 },
    Font { code: "es", gfx: 0x8e8000, tiles: 256, widths_addr: 0x8ecadf, chars: 99 },
    Font { code: "pl", gfx: 0x8e8000, tiles: 256, widths_addr: 0x8ecadf, chars: 99 },
    Font { code: "pt", gfx: 0x8e8000, tiles: 256, widths_addr: 0x8ecadf, chars: 121 },
    Font { code: "redux", gfx: 0x8e8000, tiles: 256, widths_addr: 0x8ecadf, chars: 99 },
    Font { code: "nl", gfx: 0x8e8000, tiles: 256, widths_addr: 0x8ecadf, chars: 99 },
    Font { code: "sv", gfx: 0x8e8000, tiles: 256, widths_addr: 0x8ecadf, chars: 99 },
];
