import React, { useState } from 'react'
import { Action, ActionPanel, Form, showToast, Toast } from '@raycast/api'
export default function FixtureForm(props: {arguments?: {original?: string}}) {
  const [error, setError] = useState<string | undefined>()
  return <Form navigationTitle="Fixture Form" actions={<ActionPanel><Action.SubmitForm title="Submit Fixture" onSubmit={async values => {
    if (!values.text) { setError('Text is required'); return }
    setError(undefined)
    await showToast({ style: Toast.Style.Success, title: 'Submitted', message: JSON.stringify({ ...values, password: '<redacted>' }) })
  }} /></ActionPanel>}>
    <Form.TextField id="text" title="Text" placeholder="Required text" error={error} />
    <Form.TextArea id="area" title="Multiline" defaultValue={'Line one\nLine two'} />
    <Form.PasswordField id="password" title="Password" />
    <Form.Checkbox id="checked" title="Checkbox" label="Enabled" defaultValue={true} />
    <Form.DatePicker id="date" title="Date" defaultValue={new Date(2026, 10, 7)} />
    <Form.Dropdown id="dropdown" title="Dropdown"><Form.Dropdown.Section title="Countries"><Form.Dropdown.Item value="kz" title="Kazakhstan" /><Form.Dropdown.Item value="jp" title="Japan" /></Form.Dropdown.Section></Form.Dropdown>
    <Form.TagPicker id="tags" title="Tags"><Form.TagPicker.Item value="native" title="Native" /><Form.TagPicker.Item value="bun" title="Bun" /></Form.TagPicker>
    <Form.FilePicker id="files" title="Files" allowMultipleSelection />
    <Form.Description title="Description" text={`All field kinds use the actual SDK serializer: ${props.arguments?.original ?? "missing"}`} />
    <Form.Separator />
  </Form>
}
