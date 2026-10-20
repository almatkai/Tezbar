import json, sys, traceback
try:
    from moonshine_voice import Transcriber, ModelArch
    try:
        from moonshine_voice.utils import load_wav_file
    except ImportError:
        from moonshine_voice import load_wav_file

    wav_path = sys.argv[1]
    model_path = sys.argv[2]
    language = sys.argv[3] if len(sys.argv) > 3 else "en"

    lower_path = model_path.lower()
    if "tiny" in lower_path and hasattr(ModelArch, "TINY"):
        arch = ModelArch.TINY
    elif "small" in lower_path and hasattr(ModelArch, "SMALL_STREAMING"):
        arch = ModelArch.SMALL_STREAMING
    elif hasattr(ModelArch, "BASE"):
        arch = ModelArch.BASE
    else:
        # Very old versions exposed only integer model archs.
        arch = 1

    options = {}
    if language.lower() not in ("en", "english", "es", "spanish"):
        options["max_tokens_per_second"] = "13.0"

    transcriber = Transcriber(
        model_path=model_path,
        model_arch=arch,
        options=options or None,
    )
    try:
        audio_data, sample_rate = load_wav_file(wav_path)
        transcript = transcriber.transcribe_without_streaming(
            audio_data=audio_data,
            sample_rate=sample_rate,
        )
        lines = getattr(transcript, "lines", None) or []
        text = " ".join((getattr(l, "text", "") or "").strip() for l in lines).strip()
        sys.stdout.write(text)
        sys.stdout.flush()
    finally:
        try:
            transcriber.close()
        except Exception:
            pass
except Exception as exc:
    traceback.print_exc(file=sys.stderr)
    sys.stderr.write("\
MOONSHINE_ERROR: " + repr(exc) + "\
")
    sys.exit(1)
