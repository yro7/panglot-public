#!/usr/bin/env python3
"""
Panglot Python Sidecar
Long-lived process that handles IPA transcription (epitran) and TTS (edge_tts)
via a JSON-line protocol over stdin/stdout.

Protocol:
  → {"cmd": "ipa", "lang": "pol-Latn", "text": "cześć"}
  ← {"ok": true, "ipa": "t͡ʂɛɕt͡ɕ"}

  → {"cmd": "tts", "voice": "pl-PL-ZofiaNeural", "text": "cześć", "output_path": "/tmp/lc_audio/abc.mp3"}
  ← {"ok": true, "audio_file": "/tmp/lc_audio/abc.mp3"}

  → {"cmd": "quit"}
  (process exits)
"""

import sys
import json
import asyncio

# Lazy-loaded modules
_epitran_cache = {}
_edge_tts = None


def get_epitran(lang_code):
    if lang_code not in _epitran_cache:
        import epitran
        _epitran_cache[lang_code] = epitran.Epitran(lang_code)
    return _epitran_cache[lang_code]


async def handle_tts(voice, text, output_path):
    global _edge_tts
    if _edge_tts is None:
        import edge_tts as et
        _edge_tts = et

    communicate = _edge_tts.Communicate(text, voice)
    await communicate.save(output_path)
    return output_path


def handle_ipa(lang_code, text):
    epi = get_epitran(lang_code)
    return epi.transliterate(text)


def respond(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


async def main():
    print("[Sidecar] Python sidecar started, waiting for commands...", file=sys.stderr, flush=True)

    loop = asyncio.get_event_loop()

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            respond({"ok": False, "error": f"Invalid JSON: {e}"})
            continue

        cmd = req.get("cmd")

        if cmd == "quit":
            print("[Sidecar] Shutting down.", file=sys.stderr, flush=True)
            break

        elif cmd == "ipa":
            try:
                ipa = handle_ipa(req["lang"], req["text"])
                respond({"ok": True, "ipa": ipa})
            except Exception as e:
                respond({"ok": False, "error": str(e)})

        elif cmd == "tts":
            try:
                audio_file = await handle_tts(req["voice"], req["text"], req["output_path"])
                respond({"ok": True, "audio_file": audio_file})
            except Exception as e:
                respond({"ok": False, "error": str(e)})

        else:
            respond({"ok": False, "error": f"Unknown command: {cmd}"})


if __name__ == "__main__":
    asyncio.run(main())
