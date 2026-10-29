// Interface strings. Tamil leads; English supports. Keys are grouped by surface.
export type Lang = "ta" | "en";

const S = {
  // overlay
  "ov.starting": { ta: "தயாராகிறது…", en: "Getting ready…" },
  "ov.listening": { ta: "கேட்கிறேன்…", en: "Listening…" },
  "ov.transcribing": { ta: "எழுதுகிறேன்…", en: "Writing…" },
  "ov.inserting": { ta: "ஒட்டுகிறேன்…", en: "Inserting…" },
  "ov.into": { ta: "இதில் எழுதும்", en: "Typing into" },
  "ov.noApp": { ta: "எந்த app-உம் focus-இல் இல்லை. Clipboard-க்கு copy ஆகும்", en: "No app in focus. Text will be copied" },
  "ov.self": { ta: "Mozhi: எழுத வேண்டிய app-க்கு மாறுங்கள்", en: "Mozhi: switch to the app you want to type in" },
  "ov.insert": { ta: "இடு", en: "Insert" },
  "ov.cancel": { ta: "ரத்து", en: "Cancel" },
  "ov.hintInsert": { ta: "இட", en: "insert" },
  "ov.hintCancel": { ta: "ரத்து", en: "cancel" },
  "ov.placeholder": { ta: "பேசுங்கள்: தமிழ், Tanglish, English…", en: "Speak in Tamil, Tanglish or English…" },
  "ov.mode": { ta: "எழுத்து முறை", en: "Writing mode" },

  // nav
  "nav.home": { ta: "முகப்பு", en: "Home" },
  "nav.models": { ta: "மாதிரிகள்", en: "Models" },
  "nav.writing": { ta: "எழுத்து", en: "Writing" },
  "nav.shortcut": { ta: "விசை", en: "Shortcut" },
  "nav.history": { ta: "வரலாறு", en: "History" },
  "nav.permissions": { ta: "அனுமதிகள்", en: "Permissions" },
  "nav.about": { ta: "பற்றி", en: "About" },

  // home
  "home.title": { ta: "எங்கும் பேசுங்கள். எழுத்தாகும்.", en: "Speak anywhere. It types." },
  "home.lede": {
    ta: "எந்த app-இலும் Control-ஐ இருமுறை தட்டுங்கள், பேசுங்கள், மீண்டும் தட்டுங்கள் அல்லது Enter அழுத்துங்கள்.",
    en: "In any app, double-tap Control, speak, then double-tap again or press Enter.",
  },
  "home.try": { ta: "இங்கே முயற்சி செய்யுங்கள்", en: "Try it here" },
  "home.tryHint": { ta: "இந்தப் பெட்டியைக் கிளிக் செய்து, இருமுறை தட்டிப் பேசுங்கள்.", en: "Click this box, double-tap and speak." },
  "home.ready": { ta: "தயார்", en: "Ready" },
  "home.setup": { ta: "அமைக்க வேண்டியவை", en: "Finish setting up" },
  "home.model": { ta: "பேச்சு மாதிரி", en: "Speech model" },
  "home.mic": { ta: "மைக்ரோஃபோன்", en: "Microphone" },
  "home.ax": { ta: "Accessibility (ஒட்டுவதற்கு)", en: "Accessibility (to paste)" },
  "home.stats": { ta: "இதுவரை", en: "So far" },
  "home.words": { ta: "சொற்கள்", en: "words" },
  "home.dictations": { ta: "முறை", en: "dictations" },
  "home.saved": { ta: "நிமிடம் சேமிப்பு", en: "min saved" },
  "home.login": { ta: "Login செய்யும்போதே மொழி திறக்கட்டும்", en: "Open Mozhi when you log in" },
  "home.loginHint": {
    ta: "இருமுறை தட்டுவது எப்போதும் தயாராக இருக்கும். மொழி menu bar-இல் அமைதியாகக் காத்திருக்கும்.",
    en: "So the double-tap is always ready. Mozhi waits quietly in the menu bar.",
  },
  "login.title": { ta: "மொழி தானாகத் திறக்கட்டுமா?", en: "Open Mozhi automatically?" },
  "login.body": {
    ta: "கணினியில் login செய்யும்போதே மொழி தொடங்கும், அதனால் இருமுறை தட்டியதும் உடனே பேசலாம். Home-இல் எப்போது வேண்டுமானாலும் மாற்றலாம்.",
    en: "Mozhi will start when you log in, so a double-tap works straight away. You can change this any time on Home.",
  },
  "login.yes": { ta: "ஆம், தானாகத் திற", en: "Yes, open at login" },
  "login.no": { ta: "இப்போது வேண்டாம்", en: "Not now" },

  // models
  "models.title": { ta: "பேச்சு மாதிரிகள்", en: "Speech models" },
  "models.lede": {
    ta: "எல்லாம் உங்கள் கணினியிலேயே ஓடும். பெரிய மாதிரி = அதிக துல்லியம்; சிறியது = வேகம்.",
    en: "Everything runs on your computer. Bigger = more accurate, smaller = faster.",
  },
  "models.download": { ta: "பதிவிறக்கு", en: "Download" },
  "models.use": { ta: "இதைப் பயன்படுத்து", en: "Use this" },
  "models.inUse": { ta: "பயன்பாட்டில்", en: "In use" },
  "models.delete": { ta: "நீக்கு", en: "Delete" },
  "models.cancel": { ta: "நிறுத்து", en: "Cancel" },
  "models.speed": { ta: "வேகம்", en: "Speed" },
  "models.tamil": { ta: "தமிழ் துல்லியம்", en: "Tamil accuracy" },
  "models.recommended": { ta: "பரிந்துரை", en: "Recommended" },
  "models.custom": {
    ta: "உங்கள் சொந்த ggml மாதிரியை (.bin) models கோப்புறையில் வைத்தால் இங்கே தோன்றும்.",
    en: "Drop your own ggml model (.bin) into the models folder and it appears here.",
  },

  // writing
  "writing.title": { ta: "எப்படி எழுத வேண்டும்?", en: "How should Mozhi write?" },
  "writing.polish": { ta: "உள்ளூர் AI மெருகூட்டல்", en: "Local AI polish" },
  "writing.polishLede": {
    ta: "Ollama மூலம் உங்கள் கணினியில் ஓடும் இலவச LLM. நிறுத்தக்குறிகளைச் சரிசெய்து, ஆங்கிலச் சொற்களை ஆங்கிலத்திலேயே வைக்கும்.",
    en: "A free LLM running on your machine via Ollama. It fixes punctuation and keeps English words in English.",
  },
  "writing.endpoint": { ta: "Ollama முகவரி", en: "Ollama address" },
  "writing.llm": { ta: "LLM மாதிரி", en: "LLM model" },
  "writing.test": { ta: "இணைப்பைச் சோதி", en: "Test connection" },
  "writing.behaviour": { ta: "நடத்தை", en: "Behaviour" },
  "writing.livePreview": { ta: "பேசும்போதே எழுத்தைக் காட்டு", en: "Show text while I speak" },
  "writing.autoPaste": { ta: "தானாக app-இல் ஒட்டு", en: "Paste into the app automatically" },
  "writing.restore": { ta: "பழைய clipboard-ஐ மீட்டமை", en: "Restore my clipboard afterwards" },
  "writing.sounds": { ta: "ஒலிக் குறிப்புகள்", en: "Sound cues" },
  "writing.mic": { ta: "மைக்ரோஃபோன்", en: "Microphone" },
  "writing.micDefault": { ta: "கணினியின் இயல்புநிலை", en: "System default" },
  "writing.history": { ta: "வரலாற்றைச் சேமி", en: "Keep history" },

  // shortcut
  "shortcut.title": { ta: "இருமுறை தட்டும் விசை", en: "Double-tap key" },
  "shortcut.lede": {
    ta: "இந்த விசையைத் தனியாக இருமுறை தட்டினால் Mozhi கேட்கத் தொடங்கும். Ctrl+C போன்ற சேர்க்கைகள் கணக்கில் வராது.",
    en: "Tap this key twice on its own to start. Combinations like Ctrl+C never count.",
  },
  "shortcut.speed": { ta: "இரு தட்டுகளுக்கு இடைவெளி", en: "Gap between taps" },
  "shortcut.enter": { ta: "Enter அழுத்தினாலும் இடு", en: "Enter also inserts" },
  "shortcut.position": { ta: "Overlay இடம்", en: "Overlay position" },
  "shortcut.resetPos": { ta: "கீழ் நடுவுக்கு மீட்டமை", en: "Reset to bottom centre" },

  // history
  "history.title": { ta: "வரலாறு", en: "History" },
  "history.empty": { ta: "இன்னும் எதுவும் இல்லை. Control-ஐ இருமுறை தட்டிப் பேசுங்கள்.", en: "Nothing yet. Double-tap Control and speak." },
  "history.search": { ta: "தேடு…", en: "Search…" },
  "history.copy": { ta: "நகலெடு", en: "Copy" },
  "history.copied": { ta: "நகலெடுக்கப்பட்டது", en: "Copied" },
  "history.clear": { ta: "அனைத்தையும் அழி", en: "Clear all" },
  "history.confirmClear": { ta: "உறுதியாக? மீண்டும் அழுத்துங்கள்", en: "Sure? Click again" },
  "history.local": { ta: "இந்த வரலாறு உங்கள் கணினியில் மட்டுமே உள்ளது.", en: "History lives only on this computer." },

  // permissions
  "perm.title": { ta: "அனுமதிகள்", en: "Permissions" },
  "perm.mic": { ta: "மைக்ரோஃபோன்", en: "Microphone" },
  "perm.micWhy": { ta: "நீங்கள் dictate செய்யும்போது மட்டும் கேட்க.", en: "To listen, only while you dictate." },
  "perm.ax": { ta: "Accessibility", en: "Accessibility" },
  "perm.axWhy": {
    ta: "Control இருமுறை தட்டுவதை அறிய, நீங்கள் எந்த app-இல் இருக்கிறீர்கள் என்று பார்க்க, எழுத்தை ஒட்ட.",
    en: "To notice the double-tap, see which app you're in, and paste.",
  },
  "perm.grant": { ta: "அனுமதி கொடு", en: "Allow" },
  "perm.open": { ta: "Settings-ஐத் திற", en: "Open Settings" },
  "perm.granted": { ta: "அனுமதிக்கப்பட்டது", en: "Allowed" },
  "perm.notNeeded": { ta: "தேவையில்லை", en: "Not needed" },
  "hist.copied": { ta: "நகல் மட்டும்", en: "Copied only" },
  "perm.missing": { ta: "தேவை", en: "Needed" },

  // about
  "about.title": { ta: "ஃ · மூன்று புள்ளிகள்", en: "ஃ · three dots" },
  "about.story": {
    ta: "ஆய்தம்: தமிழுக்கே உரிய எழுத்து. மூன்று புள்ளிகள்: சேர, சோழ, பாண்டிய மூவேந்தர்; கோலத்தின் புள்ளிகள்; உங்கள் குரலின் மூன்று துடிப்புகள்.",
    en: "Āytham, a letter only Tamil has. Three dots: the three crowned kings, Chera, Chola and Pandya; the pulli of a kolam; the three beats of your voice.",
  },
  "about.open": { ta: "திறந்த மூலம் · Apache-2.0", en: "Open source · Apache-2.0" },
  "about.private": { ta: "கணக்கு இல்லை. Cloud இல்லை. உங்கள் குரல் உங்கள் கணினியை விட்டு வெளியேறாது.", en: "No account. No cloud. Your voice never leaves your computer." },

  // onboarding
  "ob.welcome": { ta: "வணக்கம்!", en: "Vanakkam!" },
  "ob.welcomeLede": {
    ta: "Mozhi உங்கள் தமிழ்ப் பேச்சை எந்த app-இலும் எழுத்தாக்கும், முழுவதும் உங்கள் கணினியிலேயே.",
    en: "Mozhi turns your spoken Tamil into text in any app, entirely on your computer.",
  },
  "ob.start": { ta: "தொடங்குவோம்", en: "Let's begin" },
  "ob.next": { ta: "அடுத்து", en: "Next" },
  "ob.skip": { ta: "பிறகு", en: "Later" },
  "ob.done": { ta: "முடிந்தது. பேசுங்கள்!", en: "Done. Start talking!" },
  "ob.step1": { ta: "அனுமதிகள்", en: "Permissions" },
  "ob.step2": { ta: "மாதிரியைப் பதிவிறக்கு", en: "Download a model" },
  "ob.step3": { ta: "முயற்சி செய்", en: "Try it" },

  "common.on": { ta: "ஆன்", en: "On" },
  "common.off": { ta: "ஆஃப்", en: "Off" },
  "common.ok": { ta: "சரி", en: "OK" },
  "common.save": { ta: "சேமி", en: "Save" },
  "common.langToggle": { ta: "English", en: "தமிழ்" },
} as const;

export type Key = keyof typeof S;

let lang: Lang = "ta";
export const setLang = (l: Lang) => {
  lang = l;
  document.documentElement.lang = l;
};
export const getLang = () => lang;
export const t = (k: Key) => S[k][lang];
/** Both languages, primary first: for headings that carry the bilingual voice. */
export const tt = (k: Key) => ({ primary: S[k][lang], secondary: S[k][lang === "ta" ? "en" : "ta"] });
