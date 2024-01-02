# OpenBSD installer / uninstaller

.PHONY: install user uninstall

install: user /var/log/sarascriptd /etc/sarascriptd.conf /etc/rc.d/sarascriptd.rc.d /usr/local/sbin/sarascriptd

user:
	useradd -b /nonexistent -c "SaraScript Server Daemon" -g=uid -L daemon -s /sbin/nologin -r 100..999 sarascriptd

/var/log/sarascriptd:
	mkdir /var/log/sarascriptd
	chmod -R 0775 /var/log/sarascriptd
	chown -R sarascriptd /var/log/sarascriptd

/etc/sarascriptd.conf: sarascriptd.conf
	cp sarascriptd.conf /etc/sarascriptd.conf
	chmod 0664 /etc/sarascriptd.conf
	chown sarascriptd /etc/sarascriptd.conf

/etc/rc.d/sarascriptd.rc.d: sarascriptd.rc.d
	cp sarascriptd.rc.d /etc/rc.d/sarascriptd.rc.d
	chmod 0555 /etc/rc.d/sarascriptd.rc.d

/usr/local/sbin/sarascriptd: sarascriptd
	cp sarascriptd /usr/local/sbin/sarascriptd

uninstall:
	rm -r /var/log/sarascriptd
	rm /etc/sarascriptd.conf
	rm /etc/rc.d/sarascriptd.rc.d
	rm /usr/local/sbin/sarascriptd
	userdel sarascriptd