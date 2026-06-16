export function init(
  formSubmit,
  selHgi,
  size_min,
  size_min_txt,
  size_max,
  size_max_txt,
) {
  const es = formSubmit.elements;
  const r =
    /^(?:[^\x00-\x1F\x7F-\u009F\uE000-\uF8FF\u{F0000}-\u{FFFFD}\u{100000}-\u{10FFFD}]*)$/u;
  es.signature.oninput = (e) => {
    const i = e.target;
    if (!r.test(i.value)) {
      i.setCustomValidity("character isn't supported");
    } else {
      i.setCustomValidity("");
    }
  };

  es.comment.oninput = (e) => {
    const i = e.target;
    const encoded = new TextEncoder().encode(i.value);
    if (encoded.length > 256) {
      const percent = ((encoded.length / 256) * 100 - 100).toFixed(0);
      i.setCustomValidity(`备注过长, 超出${percent}%`);
    } else {
      i.setCustomValidity("");
    }
  };

  es.file.oninput = (e) => {
    const i = e.target;
    const f = i.files[0];
    if (f && (f.size < size_min || f.size > size_max)) {
      i.setCustomValidity(
        `不支持的文件大小, 请选择大小在 ${size_min_txt} \
和 ${size_max_txt} 之间的文件`,
      );
    } else {
      i.setCustomValidity("");
    }
  };

  let prevHgi = false;
  selHgi.onchange = () => {
    prevHgi = !prevHgi;
    const s = prevHgi ? [false, true] : [true, false];
    es.hgi.value = s[0] ? "0" : "1";
    return s;
  };
}
