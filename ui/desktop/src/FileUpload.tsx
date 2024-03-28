import { XMarkIcon, PhotoIcon } from "@heroicons/react/24/outline"
import classNames from "classnames"
import React from "react"

export interface FileUploadProps {
  onChange: (files: FileList | null) => void
  files: FileList | null
  validation?: (v: FileList | null) => string | undefined
  showValidation?: boolean
}

export default function FileUpload(props: FileUploadProps) {
  const { onChange, files, validation, showValidation } = props
  const validationMessage = validation ? validation(files) : undefined
  const valid = validationMessage === undefined

  // drag state
  const [dragActive, setDragActive] = React.useState(false)

  // handle drag events
  const handleDrag = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    if (e.type === "dragenter" || e.type === "dragover") {
      setDragActive(true)
    } else if (e.type === "dragleave") {
      setDragActive(false)
    }
  }

  const handleDragOver = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
  }

  // triggers when file is dropped
  const handleDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    setDragActive(false)
    onChange(e.dataTransfer.files)
  }

  return (
    <div
      onDragEnter={handleDrag}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
      className={classNames(
        dragActive
          ? "border-indigo-600 border-solid"
          : "border-gray-900/25 border-dashed",
        showValidation &&
          !valid &&
          "ring-red-500 focus:ring-red-500 dark:ring-red-500 dark:focus:ring-red-500",
        "mt-2 flex justify-center rounded-lg border px-6 py-10 sm:max-wd-md relative",
      )}
    >
      {files?.length && (
        <XMarkIcon
          className="absolute right-4 top-4 h-6 w-6 cursor-pointer text-gray-300"
          aria-hidden="true"
          onClick={() => onChange(null)}
        />
      )}
      <div className="text-center">
        {files?.length ? (
          <h3 className="text-center text-3xl font-bold">
            {files.length === 1
              ? `${files.length} file`
              : `${files.length} files`}
          </h3>
        ) : (
          <PhotoIcon
            className="mx-auto h-12 w-12 text-gray-300"
            aria-hidden="true"
          />
        )}
        <div className="mt-4 flex text-sm leading-6 text-gray-600">
          <label
            htmlFor="file-upload"
            className="relative cursor-pointer rounded-md bg-white font-semibold text-indigo-600 focus-within:outline-none focus-within:ring-2 focus-within:ring-indigo-600 focus-within:ring-offset-2 hover:text-indigo-500 dark:bg-transparent"
          >
            <span>Select files</span>
            <input
              id="file-upload"
              name="file-upload"
              type="file"
              multiple={true}
              className="sr-only"
              onChange={(e) => {
                onChange(e.target.files)
              }}
            />
          </label>
          <p className="pl-1">or drag and drop</p>
        </div>
        <p className="text-xs leading-5 text-gray-600">
          any format totalling a max of 1GB
        </p>
        {showValidation && validation && (
          <p className="mt-2 text-xs text-red-500">{validationMessage}</p>
        )}
      </div>
    </div>
  )
}
