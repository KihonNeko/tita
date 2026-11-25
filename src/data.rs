//! Contenus d'entraînement : mots, indicatifs, abréviations, échanges radio.

use crate::config::Lang;
use rand::Rng;

/// Mots français fréquents, sans accents ni cédille.
const FR: &str = "
LE LA LES UN UNE DES DU DE ET EN IL ELLE ON NOUS VOUS ILS JE TU ME TE SE CE CES
MON TON SON MA TA SA MES TES SES NOTRE VOTRE LEUR QUI QUE QUOI DONT OU NE PAS
PLUS MOINS TRES BIEN MAL OUI NON SI MAIS DONC OR NI CAR AVEC SANS POUR PAR SUR
SOUS DANS ENTRE VERS CHEZ AVANT APRES DEPUIS PENDANT CONTRE COMME QUAND ALORS
AUSSI ENCORE DEJA JAMAIS TOUJOURS SOUVENT PARFOIS ICI VOILA VOICI TOUT TOUS
TOUTE TOUTES RIEN PEU BEAUCOUP TROP ASSEZ AUTRE AUTRES MEME CHAQUE QUELQUE
AUCUN CECI CELA CA CELUI CELLE CEUX
ETRE AVOIR FAIRE DIRE ALLER VOIR SAVOIR POUVOIR VOULOIR VENIR DEVOIR PRENDRE
TROUVER DONNER PARLER AIMER PASSER METTRE CROIRE TENIR PORTER RESTER PENSER
SUIVRE VIVRE SORTIR PARTIR ARRIVER ENTRER MONTER TOMBER LIRE ECRIRE OUVRIR
FERMER JOUER MANGER BOIRE DORMIR COURIR MARCHER CHANTER DANSER ECOUTER
REGARDER ATTENDRE ENTENDRE RENDRE PERDRE GAGNER CHERCHER APPELER DEMANDER
REPONDRE MONTRER TOURNER COMPTER
EST SONT ETAIT SUIS ES SOMMES ETES AI AS ONT AVONS AVEZ AVAIT FAIT FAIS VA VAIS
VONT ALLEZ PEUT PEUX VEUT VEUX DOIT DOIS SAIT SAIS VOIT VOIS DIT DIS VIENT
VIENS PREND MET
HOMME FEMME ENFANT FILLE GARCON PERE MERE FRERE SOEUR FILS AMI AMIE GENS MONDE
VIE MORT JOUR NUIT MATIN SOIR MIDI HEURE MINUTE TEMPS AN ANNEE MOIS SEMAINE
HIER DEMAIN
MAISON PORTE FENETRE TABLE CHAISE LIT MUR SOL TOIT JARDIN RUE VILLE VILLAGE
PAYS ROUTE CHEMIN PONT PLACE ECOLE BUREAU TRAVAIL ARGENT PRIX
EAU FEU AIR TERRE MER CIEL SOLEIL LUNE ETOILE VENT PLUIE NEIGE NUAGE ARBRE
FLEUR FORET BOIS HERBE FEUILLE PIERRE SABLE MONTAGNE RIVIERE LAC ILE
CHIEN CHAT CHEVAL OISEAU POISSON VACHE LOUP OURS LAPIN SOURIS
TETE MAIN PIED BRAS JAMBE OEIL YEUX NEZ BOUCHE DENT COEUR CORPS DOS VOIX
PAIN VIN LAIT CAFE THE SEL SUCRE FRUIT POMME VIANDE SOUPE REPAS
ROUGE BLEU VERT JAUNE NOIR BLANC GRIS ROSE BRUN
GRAND PETIT GROS LONG COURT HAUT BAS BON BEAU BELLE JOLI NOUVEAU VIEUX JEUNE
CHAUD FROID FORT FAIBLE VITE LENT DUR DOUX PLEIN VIDE VRAI FAUX SEUL LIBRE
FACILE
DEUX TROIS QUATRE CINQ SIX SEPT HUIT NEUF DIX CENT MILLE ZERO PREMIER DERNIER
CHOSE MOT NOM IDEE HISTOIRE LIVRE PAGE LETTRE PAPIER IMAGE MUSIQUE CHANSON
FILM JEU SPORT VOITURE TRAIN AVION BATEAU VELO BUS
RADIO ONDE SIGNAL ANTENNE CODE MORSE POINT TRAIT BRUIT MESSAGE STATION BANDE
PAIX GUERRE ROI REINE LOI ETAT DROIT FORCE JOIE PEUR AMOUR RIRE REVE ESPOIR
OUEST NORD SUD GAUCHE DROITE LOIN PRES DEBOUT DEHORS DEDANS DESSUS
MERCI BONJOUR SALUT BONSOIR PARDON ENSUITE ENFIN PUIS BIENTOT PRESQUE VRAIMENT
ENSEMBLE SURTOUT PARTOUT AILLEURS LUNDI MARDI JEUDI SAMEDI ETE HIVER AUTOMNE
QUEL QUELLE POURQUOI COMMENT COMBIEN
COTE FOIS FIN DEBUT MILIEU PART PARTIE FACON CAS CAUSE EFFET RAISON SENS FORME
GROUPE NOMBRE LIGNE CERCLE CLE SAC BOITE VERRE TASSE LAMPE FIL CORDE ROUE OUTIL
";

/// Mots anglais fréquents.
const EN: &str = "
THE OF AND TO IN IS YOU THAT IT HE WAS FOR ON ARE AS WITH HIS THEY AT BE THIS
HAVE FROM OR ONE HAD BY WORD BUT NOT WHAT ALL WERE WE WHEN YOUR CAN SAID THERE
USE AN EACH WHICH SHE DO HOW THEIR IF WILL UP OTHER ABOUT OUT MANY THEN THEM
THESE SO SOME HER WOULD MAKE LIKE HIM INTO TIME HAS LOOK TWO MORE WRITE GO SEE
NUMBER NO WAY COULD PEOPLE MY THAN FIRST WATER BEEN CALL WHO OIL ITS NOW FIND
LONG DOWN DAY DID GET COME MADE MAY PART OVER NEW SOUND TAKE ONLY LITTLE WORK
KNOW PLACE YEAR LIVE ME BACK GIVE MOST VERY AFTER THING OUR JUST NAME GOOD
SENTENCE MAN THINK SAY GREAT WHERE HELP THROUGH MUCH BEFORE LINE RIGHT TOO
MEAN OLD ANY SAME TELL BOY FOLLOW CAME WANT SHOW ALSO AROUND FORM THREE SMALL
SET PUT END DOES ANOTHER WELL LARGE MUST BIG EVEN SUCH BECAUSE TURN HERE WHY
ASK WENT MEN READ NEED LAND HOME US MOVE TRY KIND HAND PICTURE AGAIN CHANGE
OFF PLAY SPELL AIR AWAY ANIMAL HOUSE POINT PAGE LETTER MOTHER ANSWER FOUND
STUDY STILL LEARN SHOULD WORLD HIGH EVERY NEAR ADD FOOD BETWEEN OWN BELOW
COUNTRY PLANT LAST SCHOOL FATHER KEEP TREE NEVER START CITY EARTH EYE LIGHT
THOUGHT HEAD UNDER STORY SAW LEFT FEW WHILE ALONG MIGHT CLOSE SEEM NEXT HARD
OPEN EXAMPLE BEGIN LIFE ALWAYS THOSE BOTH PAPER TOGETHER GOT GROUP OFTEN RUN
CHILDREN SIDE FEET CAR MILE NIGHT WALK WHITE SEA BEGAN GROW TOOK RIVER FOUR
CARRY STATE ONCE BOOK HEAR STOP WITHOUT SECOND LATER MISS IDEA ENOUGH EAT FACE
WATCH FAR REAL ALMOST LET ABOVE GIRL MOUNTAIN CUT YOUNG TALK SOON LIST SONG
BEING LEAVE FAMILY BODY MUSIC COLOR STAND SUN QUESTION FISH AREA MARK DOG
HORSE BIRDS PROBLEM COMPLETE ROOM KNEW SINCE EVER PIECE TOLD USUALLY FRIENDS
EASY HEARD ORDER RED DOOR SURE BECOME TOP SHIP ACROSS TODAY DURING SHORT
BETTER BEST HOWEVER LOW HOURS BLACK PRODUCT HAPPENED WHOLE MEASURE REMEMBER
EARLY WAVES REACHED LISTEN WIND ROCK SPACE COVERED FAST SEVERAL HOLD HIMSELF
TOWARD FIVE STEP MORNING PASSED VOWEL TRUE HUNDRED AGAINST PATTERN NUMERAL
TABLE NORTH SLOWLY MONEY MAP FARM PULLED DRAW VOICE SEEN COLD CRIED PLAN
NOTICE SOUTH SING WAR GROUND FALL KING TOWN UNIT FIGURE CERTAIN FIELD TRAVEL
WOOD FIRE UPON RADIO SIGNAL CODE KEY TONE
";

/// Mots fréquents de la langue, en MAJUSCULES sans accents, composés
/// uniquement de caractères présents dans `allowed`. Peut être vide.
pub fn words(lang: Lang, allowed: &[char]) -> Vec<&'static str> {
    let list: &'static str = match lang {
        Lang::Fr => FR,
        Lang::En => EN,
    };
    list.split_whitespace()
        .filter(|w| w.chars().all(|c| allowed.contains(&c)))
        .collect()
}

/// Abréviations et codes Q radioamateur : (code, signification) dans la
/// langue de l'interface.
pub fn abbreviations() -> &'static [(&'static str, &'static str)] {
    if crate::i18n::is_en() { ABBR_EN } else { ABBR_FR }
}

/// Mêmes codes, dans le même ordre, que `ABBR_EN`.
static ABBR_FR: &[(&str, &str)] = &[
    ("CQ", "appel général à toutes les stations"),
    ("DE", "de (ici, suivi de ton indicatif)"),
    ("K", "à toi, transmets"),
    ("KN", "à toi seulement, les autres attendent"),
    ("AR", "fin de message"),
    ("SK", "fin du contact"),
    ("BK", "break, je te repasse la main"),
    ("R", "bien reçu"),
    ("73", "amitiés"),
    ("88", "bises"),
    ("RST", "report : lisibilité, force, tonalité"),
    ("5NN", "report 599, signal parfait"),
    ("UR", "ton, ta"),
    ("TNX", "merci"),
    ("TU", "merci à toi"),
    ("FB", "très bien, parfait"),
    ("OM", "cher ami (opérateur)"),
    ("YL", "opératrice"),
    ("GM", "bonjour (le matin)"),
    ("GA", "bon après-midi"),
    ("GE", "bonsoir"),
    ("GN", "bonne nuit"),
    ("GL", "bonne chance"),
    ("HR", "ici"),
    ("HW", "comment me reçois-tu"),
    ("NR", "numéro"),
    ("NW", "maintenant"),
    ("PSE", "s'il te plaît"),
    ("AGN", "répète, encore une fois"),
    ("RPT", "répète"),
    ("CFM", "je confirme"),
    ("ANT", "antenne"),
    ("RIG", "équipement de la station"),
    ("PWR", "puissance"),
    ("WX", "météo"),
    ("ES", "et"),
    ("FER", "pour"),
    ("VY", "très"),
    ("CU", "au revoir, à bientôt"),
    ("CUL", "à plus tard"),
    ("DX", "contact à longue distance"),
    ("HI", "rire"),
    ("OP", "opérateur"),
    ("NAME", "prénom"),
    ("ABT", "environ"),
    ("B4", "déjà contacté avant"),
    ("CW", "télégraphie"),
    ("TX", "émetteur"),
    ("RX", "récepteur"),
    ("SRI", "désolé"),
    ("QTH", "ma position est"),
    ("QRZ", "qui m'appelle"),
    ("QRM", "brouillage par d'autres stations"),
    ("QRN", "parasites atmosphériques"),
    ("QSB", "le signal varie (fading)"),
    ("QSL", "j'accuse réception"),
    ("QRS", "transmets plus lentement"),
    ("QRQ", "transmets plus vite"),
    ("QRP", "faible puissance"),
    ("QRO", "forte puissance"),
    ("QRT", "je cesse d'émettre"),
    ("QRV", "je suis prêt"),
    ("QRX", "attends un instant"),
    ("QSO", "contact radio"),
    ("QSY", "changement de fréquence"),
    ("QRL", "la fréquence est occupée"),
    ("QRG", "fréquence exacte"),
];

/// Significations en anglais : mêmes codes, même ordre que `ABBR_FR`.
static ABBR_EN: &[(&str, &str)] = &[
    ("CQ", "general call to all stations"),
    ("DE", "from (this is, followed by your callsign)"),
    ("K", "over, go ahead"),
    ("KN", "over to you only, others wait"),
    ("AR", "end of message"),
    ("SK", "end of contact"),
    ("BK", "break, back to you"),
    ("R", "received"),
    ("73", "best regards"),
    ("88", "love and kisses"),
    ("RST", "report: readability, strength, tone"),
    ("5NN", "report 599, perfect signal"),
    ("UR", "your"),
    ("TNX", "thanks"),
    ("TU", "thank you"),
    ("FB", "fine business, excellent"),
    ("OM", "old man (operator)"),
    ("YL", "young lady (female operator)"),
    ("GM", "good morning"),
    ("GA", "good afternoon"),
    ("GE", "good evening"),
    ("GN", "good night"),
    ("GL", "good luck"),
    ("HR", "here"),
    ("HW", "how do you copy me"),
    ("NR", "number"),
    ("NW", "now"),
    ("PSE", "please"),
    ("AGN", "again, say it once more"),
    ("RPT", "repeat"),
    ("CFM", "I confirm"),
    ("ANT", "antenna"),
    ("RIG", "station equipment"),
    ("PWR", "power"),
    ("WX", "weather"),
    ("ES", "and"),
    ("FER", "for"),
    ("VY", "very"),
    ("CU", "goodbye, see you"),
    ("CUL", "see you later"),
    ("DX", "long-distance contact"),
    ("HI", "laughter"),
    ("OP", "operator"),
    ("NAME", "first name"),
    ("ABT", "about"),
    ("B4", "already worked before"),
    ("CW", "telegraphy"),
    ("TX", "transmitter"),
    ("RX", "receiver"),
    ("SRI", "sorry"),
    ("QTH", "my location is"),
    ("QRZ", "who is calling me"),
    ("QRM", "interference from other stations"),
    ("QRN", "atmospheric noise"),
    ("QSB", "the signal is fading"),
    ("QSL", "I acknowledge receipt"),
    ("QRS", "send more slowly"),
    ("QRQ", "send faster"),
    ("QRP", "low power"),
    ("QRO", "high power"),
    ("QRT", "I am going off the air"),
    ("QRV", "I am ready"),
    ("QRX", "wait a moment"),
    ("QSO", "radio contact"),
    ("QSY", "change of frequency"),
    ("QRL", "the frequency is in use"),
    ("QRG", "exact frequency"),
];

struct Country {
    /// Préfixes d'indicatif ; un préfixe finissant par un chiffre (HB9) ne
    /// reçoit pas de chiffre supplémentaire.
    prefixes: &'static [&'static str],
    names: &'static [&'static str],
    cities: &'static [&'static str],
}

const COUNTRIES: &[Country] = &[
    Country {
        prefixes: &["F"],
        names: &["PIERRE", "JEAN", "MARC", "ALAIN", "MICHEL", "CLAIRE", "ANNE", "LUC"],
        cities: &["PARIS", "LYON", "NANTES", "LILLE", "BREST", "TOULOUSE", "BORDEAUX", "DIJON"],
    },
    Country {
        prefixes: &["DL", "DJ", "DK"],
        names: &["HANS", "KARL", "PETER", "UWE", "JURGEN", "HEIKE"],
        cities: &["BERLIN", "MUNICH", "HAMBURG", "BONN", "DRESDEN"],
    },
    Country {
        prefixes: &["G", "M"],
        names: &["JOHN", "DAVE", "TOM", "STEVE", "MARY"],
        cities: &["LONDON", "LEEDS", "BRISTOL", "YORK", "OXFORD"],
    },
    Country {
        prefixes: &["EA"],
        names: &["LUIS", "JOSE", "PEDRO", "CARMEN"],
        cities: &["MADRID", "SEVILLA", "VALENCIA", "BILBAO"],
    },
    Country {
        prefixes: &["I", "IK", "IZ"],
        names: &["MARIO", "LUCA", "PAOLO", "ANNA"],
        cities: &["ROMA", "MILANO", "TORINO", "NAPOLI"],
    },
    Country {
        prefixes: &["ON"],
        names: &["JAN", "LUC", "MARC", "EMMA"],
        cities: &["BRUXELLES", "LIEGE", "GENT", "NAMUR"],
    },
    Country {
        prefixes: &["HB9"],
        names: &["URS", "RETO", "DANIEL", "HEIDI"],
        cities: &["GENEVE", "BERN", "ZURICH", "LAUSANNE"],
    },
    Country {
        prefixes: &["K", "W", "N", "AA", "KB", "WA"],
        names: &["BOB", "MIKE", "JIM", "BILL", "SUE", "DAN"],
        cities: &["BOSTON", "DENVER", "DALLAS", "SEATTLE", "CHICAGO"],
    },
    Country {
        prefixes: &["JA", "JH"],
        names: &["KENJI", "TARO", "HIRO", "YUKI"],
        cities: &["TOKYO", "OSAKA", "KYOTO", "NAGOYA"],
    },
    Country {
        prefixes: &["VE", "VA"],
        names: &["PAUL", "ANDRE", "GARY", "LISE"],
        cities: &["MONTREAL", "QUEBEC", "TORONTO", "OTTAWA"],
    },
    Country {
        prefixes: &["PA"],
        names: &["JAN", "PIET", "KEES", "ANNA"],
        cities: &["AMSTERDAM", "UTRECHT", "LEIDEN"],
    },
    Country {
        prefixes: &["OE"],
        names: &["FRANZ", "JOSEF", "MARIA"],
        cities: &["WIEN", "GRAZ", "LINZ"],
    },
];

fn pick<'a, T>(rng: &mut impl Rng, items: &'a [T]) -> &'a T {
    &items[rng.random_range(0..items.len())]
}

fn ends_with_digit(s: &str) -> bool {
    s.chars().last().is_some_and(|c| c.is_ascii_digit())
}

/// Chiffres, lettres et préfixes utilisables avec `allowed` (`None` = tout).
fn call_parts(
    prefixes: impl Iterator<Item = &'static str>,
    allowed: Option<&[char]>,
) -> (Vec<&'static str>, Vec<char>, Vec<char>) {
    let ok = |c: char| allowed.is_none_or(|a| a.contains(&c));
    let digits: Vec<char> = ('0'..='9').filter(|c| ok(*c)).collect();
    let letters: Vec<char> = ('A'..='Z').filter(|c| ok(*c)).collect();
    let prefixes = prefixes
        .filter(|p| p.chars().all(ok) && (ends_with_digit(p) || !digits.is_empty()))
        .collect();
    (prefixes, digits, letters)
}

fn make_call(
    rng: &mut impl Rng,
    prefixes: impl Iterator<Item = &'static str>,
    allowed: Option<&[char]>,
) -> Option<String> {
    let (prefixes, digits, letters) = call_parts(prefixes, allowed);
    if prefixes.is_empty() || letters.is_empty() {
        return None;
    }
    let mut out = pick(rng, &prefixes).to_string();
    if !ends_with_digit(&out) {
        out.push(*pick(rng, &digits));
    }
    let n = match rng.random_range(0..100) {
        0..15 => 1,
        15..55 => 2,
        _ => 3,
    };
    for _ in 0..n {
        out.push(*pick(rng, &letters));
    }
    Some(out)
}

fn all_prefixes() -> impl Iterator<Item = &'static str> {
    COUNTRIES.iter().flat_map(|c| c.prefixes.iter().copied())
}

/// Indicatif radioamateur aléatoire plausible (ex. "F4ABC", "DL1XY").
pub fn callsign(rng: &mut impl Rng) -> String {
    make_call(rng, all_prefixes(), None).expect("tous les caractères sont permis")
}

/// Indicatif composé uniquement de caractères de `allowed` ; `None` si c'est
/// impossible (aucun préfixe, chiffre ou lettre utilisable).
pub fn callsign_from(rng: &mut impl Rng, allowed: &[char]) -> Option<String> {
    make_call(rng, all_prefixes(), Some(allowed))
}

/// Nombre de débuts d'indicatif distincts (préfixe + chiffre) réalisables avec
/// `allowed` : mesure la variété disponible.
pub fn callsign_bases(allowed: &[char]) -> usize {
    let (prefixes, digits, letters) = call_parts(all_prefixes(), Some(allowed));
    if letters.is_empty() {
        return 0;
    }
    prefixes.iter().map(|p| if ends_with_digit(p) { 1 } else { digits.len() }).sum()
}

/// Échange radio (QSO) simulé, court, en plusieurs transmissions.
pub fn qso(rng: &mut impl Rng) -> Vec<String> {
    let ca = pick(rng, COUNTRIES);
    let cb = pick(rng, COUNTRIES);
    let a = make_call(rng, ca.prefixes.iter().copied(), None).expect("indicatif");
    let mut b = make_call(rng, cb.prefixes.iter().copied(), None).expect("indicatif");
    if b == a {
        b.push('X');
    }
    if rng.random_bool(0.15) {
        b.push_str("/P");
    }
    let (name_a, name_b) = (*pick(rng, ca.names), *pick(rng, cb.names));
    let (qth_a, qth_b) = (*pick(rng, ca.cities), *pick(rng, cb.cities));
    const RST: &[&str] = &["599", "5NN", "589", "579", "569", "559"];
    let (rst_a, rst_b) = (*pick(rng, RST), *pick(rng, RST));
    let hello = *pick(rng, &["GM", "GA", "GE"]);
    let over = *pick(rng, &["HW?", "HW CPY?", "BK"]);

    let cq = format!("CQ CQ CQ DE {a} {a} K");
    let call = format!("{a} DE {b} {b} K");
    let report_a = format!(
        "{b} DE {a} = {hello} OM TNX FER CALL = UR RST {rst_b} = NAME {name_a} = QTH {qth_a} = {over} {b} DE {a} K"
    );
    let report_b = format!(
        "{a} DE {b} = R TNX {name_a} = UR RST {rst_a} = NAME {name_b} = QTH {qth_b} = {a} DE {b} K"
    );
    let bye_a = format!("{b} DE {a} = FB {name_b} TNX FER QSO = 73 ES CUL = {b} DE {a} SK");
    let bye_b = format!("{a} DE {b} = TNX {name_a} 73 = {a} DE {b} SK");
    match rng.random_range(3..=5) {
        3 => vec![cq, call, format!("{b} DE {a} = {hello} UR RST {rst_b} = NAME {name_a} = 73 = {b} DE {a} SK")],
        4 => vec![cq, call, report_a, format!("{a} DE {b} = R UR RST {rst_a} = NAME {name_b} = QTH {qth_b} = 73 ES CUL = {a} DE {b} SK")],
        _ => vec![cq, call, report_a, report_b, if rng.random_bool(0.5) { bye_a } else { bye_b }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morse;
    use std::collections::BTreeSet;

    fn in_table(s: &str) -> bool {
        s.chars().all(|c| c == ' ' || morse::TABLE.iter().any(|(k, _)| *k == c))
    }

    #[test]
    fn word_lists_are_clean() {
        let all: Vec<char> = ('A'..='Z').collect();
        for lang in [Lang::Fr, Lang::En] {
            let list = words(lang, &all);
            assert!(list.len() >= 380, "{lang:?} : {} mots", list.len());
            let mut seen = BTreeSet::new();
            let dups: Vec<_> = list.iter().filter(|w| !seen.insert(**w)).collect();
            assert!(dups.is_empty(), "{lang:?} doublons : {dups:?}");
            for w in &list {
                assert!((2..=8).contains(&w.len()), "{w}");
                assert!(w.chars().all(|c| c.is_ascii_uppercase()), "{w}");
            }
            // Rien n'est écarté quand tout l'alphabet est permis.
            let raw = match lang {
                Lang::Fr => FR,
                Lang::En => EN,
            };
            assert_eq!(list.len(), raw.split_whitespace().count());
        }
    }

    #[test]
    fn words_are_filtered_by_allowed() {
        let allowed = ['E', 'T', 'I', 'M', 'S', 'O'];
        for lang in [Lang::Fr, Lang::En] {
            let list = words(lang, &allowed);
            assert!(!list.is_empty());
            assert!(list.iter().all(|w| w.chars().all(|c| allowed.contains(&c))));
        }
        assert!(words(Lang::Fr, &allowed).contains(&"MOT"));
        assert!(!words(Lang::Fr, &allowed).contains(&"MAIN"));
        assert_eq!(words(Lang::Fr, &['E', 'T']), vec!["ET", "TE", "TETE", "ETE"]);
        assert!(words(Lang::Fr, &[]).is_empty());
    }

    #[test]
    fn abbreviations_are_playable_and_unique() {
        let list = abbreviations();
        assert!(list.len() >= 50);
        let mut seen = BTreeSet::new();
        for (code, meaning) in list {
            assert!(in_table(code) && !code.contains(' ') && !code.is_empty(), "{code}");
            assert!(!meaning.is_empty());
            assert!(seen.insert(*code), "doublon {code}");
        }
    }

    #[test]
    fn abbreviation_tables_match_in_both_languages() {
        let codes = |t: &[(&'static str, &'static str)]| t.iter().map(|(c, _)| *c).collect::<Vec<_>>();
        assert_eq!(codes(ABBR_FR), codes(ABBR_EN));
        assert!(ABBR_EN.iter().all(|(_, m)| !m.is_empty() && m.is_ascii()));
        // Même largeur d'affichage maximale, à peu près : rien de démesuré.
        assert!(ABBR_EN.iter().all(|(_, m)| m.chars().count() <= 45));
        assert_eq!(abbreviations()[0].1, ABBR_FR[0].1);
        crate::i18n::set(Lang::En);
        let en = abbreviations();
        crate::i18n::set(Lang::Fr);
        assert_eq!(en[0], ("CQ", "general call to all stations"));
        assert_eq!(codes(en), codes(abbreviations()));
        assert_ne!(en[0].1, abbreviations()[0].1);
    }

    #[test]
    fn callsigns_are_plausible() {
        let mut rng = rand::rng();
        let mut seen = BTreeSet::new();
        for _ in 0..300 {
            let c = callsign(&mut rng);
            assert!(in_table(&c) && (3..=7).contains(&c.len()), "{c}");
            assert!(c.chars().all(|x| x.is_ascii_uppercase() || x.is_ascii_digit()), "{c}");
            assert!(c.chars().any(|x| x.is_ascii_digit()), "{c}");
            assert!(c.chars().last().unwrap().is_ascii_uppercase(), "{c}");
            seen.insert(c);
        }
        assert!(seen.len() > 200);
    }

    #[test]
    fn callsign_from_respects_allowed() {
        let mut rng = rand::rng();
        let letters: Vec<char> = ('A'..='Z').collect();
        assert_eq!(callsign_from(&mut rng, &letters), None);
        assert_eq!(callsign_bases(&letters), 0);
        let allowed = ['K', 'F', 'E', 'T', '5'];
        assert_eq!(callsign_bases(&allowed), 2);
        for _ in 0..100 {
            let c = callsign_from(&mut rng, &allowed).unwrap();
            assert!(c.chars().all(|x| allowed.contains(&x)), "{c}");
        }
        // HB9 porte son propre chiffre.
        let c = callsign_from(&mut rng, &['H', 'B', '9']).unwrap();
        assert!(c.starts_with("HB9"), "{c}");
    }

    #[test]
    fn qsos_are_playable() {
        let mut rng = rand::rng();
        for _ in 0..300 {
            let q = qso(&mut rng);
            assert!((3..=5).contains(&q.len()));
            assert!(q[0].starts_with("CQ "));
            for t in &q {
                assert!(in_table(t), "{t}");
                assert!(!t.contains("  ") && t.trim() == t, "{t}");
                assert!(t.len() <= 120, "{} : {t}", t.len());
            }
            assert!(q.iter().any(|t| t.contains("RST")));
            assert!(q.last().unwrap().contains("73"));
        }
    }
}
