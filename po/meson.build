i18n = import('i18n')
# The Rust sources use tr(), ntr(), ctr() and qtr() (see src/i18n.rs)
po_targets = i18n.gettext(
  project_name,
  args: [
    '--from-code=UTF-8',
    '--keyword=tr',
    '--keyword=ntr:1,2',
    '--keyword=ctr:1c,2',
    '--keyword=qtr',
  ],
)
