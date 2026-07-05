export function init(
  formSubmit,
  size_min,
  size_min_txt,
  size_max,
  size_max_txt,
) {
  const es = formSubmit.elements;

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
}
