import React from 'react'
import { Form } from '@raycast/api'
export default function ProbeDate() {
  return <Form navigationTitle="Probe Date">
    <Form.DatePicker id="local" title="Local midnight" defaultValue={new Date(2026, 10, 7)} />
    <Form.DatePicker id="evening" title="Local 23:30" defaultValue={new Date(2026, 10, 7, 23, 30)} />
    <Form.DatePicker id="datetime" title="Date and time" type={Form.DatePicker.Type.DateTime} defaultValue={new Date(2026, 10, 7, 23, 30)} />
  </Form>
}
