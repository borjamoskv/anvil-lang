tell application "Mail"
	set unreadMsgs to (messages of inbox whose read status is false)
	set msgList to {}
	repeat with msg in unreadMsgs
		set msgSender to sender of msg
		if msgSender contains "sealons@yahoo.es" then
			set msgSubject to subject of msg
			set msgContent to content of msg
			set end of msgList to "Subject: " & msgSubject & "\nContent: " & msgContent
			set read status of msg to true
		end if
	end repeat
	
	set text item delimiters to "\n---\n"
	return msgList as string
end tell
