import React from "react";

interface LogoProps {
  width?: number;
  height?: number;
  className?: string;
}

export function Logo({ width = 32, height = 32, className = "" }: LogoProps) {
  return (
    <div
      className={`flex items-center justify-center bg-primary text-primary-foreground rounded-lg ${className}`}
      style={{ width, height }}
    >
      <svg
        width={width * 0.7}
        height={height * 0.7}
        viewBox="0 0 24 24"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
      >
        <path
          d="M21 7.5V6.5C21 5.4 20.1 4.5 19 4.5H5C3.9 4.5 3 5.4 3 6.5V7.5M21 7.5H3M21 7.5V17.5C21 18.6 20.1 19.5 19 19.5H5C3.9 19.5 3 18.6 3 17.5V7.5"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <circle cx="7.5" cy="13.5" r="1.5" fill="currentColor" />
        <circle cx="16.5" cy="13.5" r="1.5" fill="currentColor" />
        <path
          d="M9.5 10.5H14.5"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
        />
        <path
          d="M9.5 16.5H14.5"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
        />
      </svg>
    </div>
  );
}
