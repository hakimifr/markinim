//! Regex patterns of `owoifynim` (`src/owoifynim/private/mapping.nim`),
//! copied verbatim from the Nim source along with its `FACES` list.

pub const O_TO_OWO: &str = r"o";
pub const EW_TO_UWU: &str = r"ew";
pub const HEY_TO_HAY: &str = r"([Hh])ey";
pub const DEAD_TO_DED_UPPER: &str = r"Dead";
pub const DEAD_TO_DED_LOWER: &str = r"dead";
pub const N_VOWEL_T_TO_ND: &str = r"n[aeiou]*t";
pub const READ_TO_WEAD_UPPER: &str = r"Read";
pub const READ_TO_WEAD_LOWER: &str = r"read";
pub const BRACKETS_TO_STARTRAILS_FORE: &str = r"[({<]";
pub const BRACKETS_TO_STARTRAILS_REAR: &str = r"[)}>]";
pub const PERIOD_COMMA_EXCLAMATION_SEMICOLON_TO_KAOMOJIS_FIRST: &str = r"[.,](?![0-9])";
pub const PERIOD_COMMA_EXCLAMATION_SEMICOLON_TO_KAOMOJIS_SECOND: &str = r"[!;]+";
pub const THAT_TO_DAT_UPPER: &str = r"That";
pub const THAT_TO_DAT_LOWER: &str = r"that";
pub const TH_TO_F_UPPER: &str = r"TH(?!E)";
pub const TH_TO_F_LOWER: &str = r"[Tt]h(?![Ee])";
pub const LE_TO_WAL: &str = r"le$";
pub const VE_TO_WE_UPPER: &str = r"Ve";
pub const VE_TO_WE_LOWER: &str = r"ve";
pub const RY_TO_WWY: &str = r"ry";
pub const RORL_TO_W_UPPER: &str = r"(?:R|L)";
pub const RORL_TO_W_LOWER: &str = r"(?:r|l)";
pub const LL_TO_WW: &str = r"ll";
pub const VOWEL_OR_R_EXCEPT_O_L_TO_WL_UPPER: &str = r"[AEIUR]([lL])$";
pub const VOWEL_OR_R_EXCEPT_O_L_TO_WL_LOWER: &str = r"[aeiur]l$";
pub const OLD_TO_OWLD_UPPER: &str = r"OLD";
pub const OLD_TO_OWLD_LOWER: &str = r"([Oo])ld";
pub const OL_TO_OWL_UPPER: &str = r"OL";
pub const OL_TO_OWL_LOWER: &str = r"([Oo])l";
pub const LORR_O_TO_WO_UPPER: &str = r"[LR]([oO])";
pub const LORR_O_TO_WO_LOWER: &str = r"[lr]o";
pub const SPECIFIC_CONSONANTS_O_TO_LETTER_AND_WO_UPPER: &str = r"([BCDFGHJKMNPQSTXYZ])([oO])";
pub const SPECIFIC_CONSONANTS_O_TO_LETTER_AND_WO_LOWER: &str = r"([bcdfghjkmnpqstxyz])o";
pub const VORW_LE_TO_WAL: &str = r"[vw]le";
pub const FI_TO_FWI_UPPER: &str = r"FI";
pub const FI_TO_FWI_LOWER: &str = r"([Ff])i";
pub const VER_TO_WER: &str = r"([Vv])er";
pub const POI_TO_PWOI: &str = r"([Pp])oi";
pub const SPECIFIC_CONSONANTS_LE_TO_LETTER_AND_WAL: &str = r"([DdFfGgHhJjPpQqRrSsTtXxYyZz])le$";
pub const CONSONANT_R_TO_CONSONANT_W: &str = r"([BbCcDdFfGgKkPpQqSsTtWwXxZz])r";
pub const LY_TO_WY_UPPER: &str = r"Ly";
pub const LY_TO_WY_LOWER: &str = r"ly";
pub const PLE_TO_PWE: &str = r"([Pp])le";
pub const NR_TO_NW_UPPER: &str = r"NR";
pub const NR_TO_NW_LOWER: &str = r"nr";
pub const FUC_TO_FWUC: &str = r"([Ff])uc";
pub const MOM_TO_MWOM: &str = r"([Mm])om";
pub const ME_TO_MWE: &str = r"([Mm])e";
pub const N_VOWEL_TO_NY_FIRST: &str = r"n([aeiou])";
pub const N_VOWEL_TO_NY_SECOND: &str = r"N([aeiou])";
pub const N_VOWEL_TO_NY_THIRD: &str = r"N([AEIOU])";
pub const OVE_TO_UV_UPPER: &str = r"OVE";
pub const OVE_TO_UV_LOWER: &str = r"ove";
pub const HAHA_TO_HEHE_XD: &str = r"\b(ha|hah|heh|hehe)+\b";
pub const THE_TO_TEH: &str = r"\b([Tt])he\b";
pub const YOU_TO_U_UPPER: &str = r"\bYou\b";
pub const YOU_TO_U_LOWER: &str = r"\byou\b";
pub const TIME_TO_TIM: &str = r"\b([Tt])ime\b";
pub const OVER_TO_OWOR: &str = r"([Oo])ver";
pub const WORSE_TO_WOSE: &str = r"([Ww])orse";

pub const ALL: [&str; 60] = [
    O_TO_OWO,
    EW_TO_UWU,
    HEY_TO_HAY,
    DEAD_TO_DED_UPPER,
    DEAD_TO_DED_LOWER,
    N_VOWEL_T_TO_ND,
    READ_TO_WEAD_UPPER,
    READ_TO_WEAD_LOWER,
    BRACKETS_TO_STARTRAILS_FORE,
    BRACKETS_TO_STARTRAILS_REAR,
    PERIOD_COMMA_EXCLAMATION_SEMICOLON_TO_KAOMOJIS_FIRST,
    PERIOD_COMMA_EXCLAMATION_SEMICOLON_TO_KAOMOJIS_SECOND,
    THAT_TO_DAT_UPPER,
    THAT_TO_DAT_LOWER,
    TH_TO_F_UPPER,
    TH_TO_F_LOWER,
    LE_TO_WAL,
    VE_TO_WE_UPPER,
    VE_TO_WE_LOWER,
    RY_TO_WWY,
    RORL_TO_W_UPPER,
    RORL_TO_W_LOWER,
    LL_TO_WW,
    VOWEL_OR_R_EXCEPT_O_L_TO_WL_UPPER,
    VOWEL_OR_R_EXCEPT_O_L_TO_WL_LOWER,
    OLD_TO_OWLD_UPPER,
    OLD_TO_OWLD_LOWER,
    OL_TO_OWL_UPPER,
    OL_TO_OWL_LOWER,
    LORR_O_TO_WO_UPPER,
    LORR_O_TO_WO_LOWER,
    SPECIFIC_CONSONANTS_O_TO_LETTER_AND_WO_UPPER,
    SPECIFIC_CONSONANTS_O_TO_LETTER_AND_WO_LOWER,
    VORW_LE_TO_WAL,
    FI_TO_FWI_UPPER,
    FI_TO_FWI_LOWER,
    VER_TO_WER,
    POI_TO_PWOI,
    SPECIFIC_CONSONANTS_LE_TO_LETTER_AND_WAL,
    CONSONANT_R_TO_CONSONANT_W,
    LY_TO_WY_UPPER,
    LY_TO_WY_LOWER,
    PLE_TO_PWE,
    NR_TO_NW_UPPER,
    NR_TO_NW_LOWER,
    FUC_TO_FWUC,
    MOM_TO_MWOM,
    ME_TO_MWE,
    N_VOWEL_TO_NY_FIRST,
    N_VOWEL_TO_NY_SECOND,
    N_VOWEL_TO_NY_THIRD,
    OVE_TO_UV_UPPER,
    OVE_TO_UV_LOWER,
    HAHA_TO_HEHE_XD,
    THE_TO_TEH,
    YOU_TO_U_UPPER,
    YOU_TO_U_LOWER,
    TIME_TO_TIM,
    OVER_TO_OWOR,
    WORSE_TO_WOSE,
];

#[rustfmt::skip]
pub const FACES: [&str; 29] = [
    "(・`ω´・)", ";;w;;", "owo", "UwU", ">w<", "^w^", "(* ^ ω ^)",
    "(⌒ω⌒)", "ヽ(*・ω・)ﾉ", "(o´∀`o)", "(o･ω･o)", "＼(＾▽＾)／",
    "(*^ω^)", "(◕‿◕✿)", "(◕ᴥ◕)", "ʕ•ᴥ•ʔ", "ʕ￫ᴥ￩ʔ", "(*^.^*)", "(｡♥‿♥｡)",
    "OwO", "uwu", "uvu", "UvU", "(*￣з￣)", "(つ✧ω✧)つ", "(/ =ω=)/",
    "(╯°□°）╯︵ ┻━┻", "┬─┬ ノ( ゜-゜ノ)", "¯\\_(ツ)_/¯"];
